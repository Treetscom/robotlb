#![warn(
    // Base lints.
    clippy::all,
    // Some pedantic lints.
    clippy::pedantic,
    // New lints which are cool.
    clippy::nursery,
)]
#![
    allow(
        // I don't care about this.
        clippy::module_name_repetitions,
        // Yo, the hell you should put
        // it in docs, if signature is clear as sky.
        clippy::missing_errors_doc
    )
]

use backoff::ErrorBackoff;
use clap::Parser;
use config::OperatorConfig;
use error::{redact, RobotLBError, RobotLBResult};
use futures::StreamExt;
use hcloud::apis::configuration::Configuration as HCloudConfig;
use k8s_openapi::{
    api::{
        core::v1::{Node, Service},
        discovery::v1::EndpointSlice,
    },
    serde_json::json,
};
use kube::{
    api::{ListParams, PatchParams},
    runtime::{
        controller::{self, Action},
        events::{Event, EventType, Recorder, Reporter},
        watcher, Controller, WatchStreamExt,
    },
    Resource, ResourceExt,
};
use label_filter::LabelFilter;
use lb::{LBService, LoadBalancer};
use rate_limit::{spread, RateLimitGate};
use std::{
    collections::HashSet,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

pub mod backoff;
pub mod config;
pub mod consts;
pub mod error;
pub mod finalizers;
pub mod label_filter;
pub mod lb;
pub mod rate_limit;
pub mod triggers;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> RobotLBResult<()> {
    dotenvy::dotenv().ok();
    let operator_config = config::OperatorConfig::parse();
    tracing_subscriber::fmt()
        .with_max_level(operator_config.log_level)
        .init();

    let mut hcloud_conf = HCloudConfig::new();
    hcloud_conf.bearer_access_token = Some(operator_config.hcloud_token.clone());

    tracing::info!("Starting robotlb operator v{}", env!("CARGO_PKG_VERSION"));
    let kube_client = kube::Client::try_default().await?;
    tracing::info!("Kube client is connected");
    let context = Arc::new(CurrentContext::new(
        kube_client.clone(),
        operator_config.clone(),
        hcloud_conf,
    ));
    tracing::info!("Starting the controller");
    let token = operator_config.hcloud_token;
    let dynamic_node_selector = operator_config.dynamic_node_selector;
    let controller = Controller::new(
        kube::Api::<Service>::all(kube_client.clone()),
        watcher::Config::default(),
    );
    let services = controller.store();
    let slice_changes = Arc::new(triggers::SliceChanges::default());
    controller
        .reconcile_all_on(cluster_changes(kube_client.clone(), slice_changes.clone()))
        .watches(
            kube::Api::<EndpointSlice>::all(kube_client),
            watcher::Config::default(),
            move |slice| {
                triggers::slice_service(&slice).filter(|service| {
                    let svc = services.get(service);
                    slice_triggers(
                        &slice,
                        svc.as_deref(),
                        dynamic_node_selector,
                        &slice_changes,
                    )
                })
            },
        )
        .run(reconcile_service, on_error, context)
        .for_each(|reconcilation_result| {
            let token = token.clone();
            async move {
                match reconcilation_result {
                    Ok((service, _action)) => {
                        tracing::info!(
                            "Reconcilation of a service {} was successful",
                            service.name
                        );
                    }
                    // During reconcilation process,
                    // the controller has decided to skip the service.
                    Err(controller::Error::ReconcilerFailed(RobotLBError::SkipService, _)) => {}
                    Err(controller::Error::ReconcilerFailed(
                        error @ RobotLBError::RateLimited(_),
                        service,
                    )) => {
                        tracing::info!("Service {service}: {error}");
                    }
                    Err(controller::Error::ReconcilerFailed(error, service)) => {
                        tracing::error!(
                            "Reconcilation of service {service} failed: {}",
                            redact(&error.to_string(), &token)
                        );
                    }
                    Err(error) => {
                        tracing::error!("Controller error: {}", redact(&error.to_string(), &token));
                    }
                }
            }
        })
        .await;
    Ok(())
}

fn is_robotlb_load_balancer(svc: &Service) -> bool {
    let spec = svc.spec.as_ref();
    spec.and_then(|spec| spec.type_.as_deref()) == Some("LoadBalancer")
        && spec
            .and_then(|spec| spec.load_balancer_class.as_deref())
            .unwrap_or(consts::ROBOTLB_LB_CLASS)
            == consts::ROBOTLB_LB_CLASS
}

/// Whether a change of the slice should reconcile its service. Only services that
/// target the nodes of their endpoints care where those run.
fn slice_triggers(
    slice: &EndpointSlice,
    svc: Option<&Service>,
    dynamic_node_selector: bool,
    changes: &triggers::SliceChanges,
) -> bool {
    let observed = svc.is_some_and(|svc| {
        is_robotlb_load_balancer(svc)
            && node_source(svc, dynamic_node_selector) == NodeSource::ServiceEndpoints
    });
    if !observed {
        changes.forget(slice);
        return false;
    }
    changes.observe(slice)
}

/// A signal each time nodes change in a way that can change balancer targets, or an
/// endpoint slice with endpoints is deleted.
fn cluster_changes(
    client: kube::Client,
    slices: Arc<triggers::SliceChanges>,
) -> impl futures::Stream<Item = ()> + Send + Sync {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    let node_sender = sender.clone();
    let node_client = client.clone();
    tokio::spawn(async move {
        let mut changes = triggers::NodeChanges::default();
        let mut events = std::pin::pin!(watcher::watcher(
            kube::Api::<Node>::all(node_client),
            watcher::Config::default()
        )
        .default_backoff());
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => {
                    if changes.observe(&event) && node_sender.unbounded_send(()).is_err() {
                        return;
                    }
                }
                Err(error) => tracing::warn!("Node watch failed: {error}"),
            }
        }
    });
    // A deleted slice triggers every service, since only the unstable
    // `Controller::reconcile_on` can target one. Once kube-runtime stabilizes it,
    // send the slice's service there instead and drop this watch's reconcile-all.
    tokio::spawn(async move {
        let mut events = std::pin::pin!(watcher::metadata_watcher(
            kube::Api::<EndpointSlice>::all(client),
            watcher::Config::default()
        )
        .default_backoff());
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => {
                    if slices.track(&event) && sender.unbounded_send(()).is_err() {
                        return;
                    }
                }
                Err(error) => tracing::warn!("Endpoint slice watch failed: {error}"),
            }
        }
    });
    receiver
}

#[derive(Clone)]
pub struct CurrentContext {
    pub client: kube::Client,
    pub config: OperatorConfig,
    pub hcloud_config: HCloudConfig,
    pub rate_limit: Arc<RateLimitGate>,
    pub error_backoff: Arc<ErrorBackoff>,
}
impl CurrentContext {
    #[must_use]
    pub fn new(client: kube::Client, config: OperatorConfig, hcloud_config: HCloudConfig) -> Self {
        Self {
            client,
            config,
            hcloud_config,
            rate_limit: Arc::default(),
            error_backoff: Arc::default(),
        }
    }
}

/// Reconcile the service.
///
/// This function is called by the controller for each service.
/// It will create or update the load balancer based on the service.
/// If the service is being deleted, it will clean up the resources.
#[tracing::instrument(skip(svc,context), fields(service=svc.name_any()))]
pub async fn reconcile_service(
    svc: Arc<Service>,
    context: Arc<CurrentContext>,
) -> RobotLBResult<Action> {
    let result = sync_service(svc.clone(), context.clone()).await;
    if ends_backoff(&result) {
        context.error_backoff.forget(&service_key(&svc));
    }
    if let Err(error) = &result {
        if publishes_event(error) {
            report_failure(&svc, &context, error).await;
        }
    }
    result
}

/// A skipped service may have failed back when it was still a robotlb load balancer.
/// A service waiting at the rate limit gate has not got any closer to succeeding.
const fn ends_backoff(result: &RobotLBResult<Action>) -> bool {
    matches!(result, Ok(_) | Err(RobotLBError::SkipService))
}

fn service_key(svc: &Service) -> String {
    format!("{}/{}", svc.namespace().unwrap_or_default(), svc.name_any())
}

/// Skipped services are every service robotlb does not own. A service waiting at the
/// rate limit gate still gets an event each time it wakes up to a closed gate.
const fn publishes_event(error: &RobotLBError) -> bool {
    !matches!(error, RobotLBError::SkipService)
}

/// Put the error on the service as a warning event, where `kubectl describe` shows it.
async fn report_failure(svc: &Service, context: &CurrentContext, error: &RobotLBError) {
    let recorder = Recorder::new(
        context.client.clone(),
        Reporter {
            controller: "robotlb".to_string(),
            instance: None,
        },
        svc.object_ref(&()),
    );
    let published = recorder
        .publish(Event {
            type_: EventType::Warning,
            reason: "SyncLoadBalancerFailed".to_string(),
            note: Some(event_note(error, &context.config.hcloud_token)),
            action: "Reconcile".to_string(),
            secondary: None,
        })
        .await;
    if let Err(publish_error) = published {
        tracing::warn!("Cannot publish an event for the service: {publish_error}");
    }
}

/// The error as an event note: token redacted and cut to the size the API accepts.
fn event_note(error: &RobotLBError, token: &str) -> String {
    const MAX_NOTE_BYTES: usize = 1024;
    let mut note = redact(&error.to_string(), token);
    if note.len() > MAX_NOTE_BYTES {
        let mut end = MAX_NOTE_BYTES;
        while !note.is_char_boundary(end) {
            end -= 1;
        }
        note.truncate(end);
    }
    note
}

#[derive(Debug, PartialEq, Eq)]
enum ServiceRole {
    /// The service is not robotlb's.
    Skip,
    /// The service needs its load balancer created or updated.
    Reconcile,
    /// The service had a load balancer and no longer needs it.
    Release,
}

/// The API server wipes `loadBalancerClass` together with the `LoadBalancer` type,
/// so once the type changes only the finalizer still marks the service as robotlb's,
/// and a service with the finalizer that robotlb no longer serves gets released.
fn service_role(svc: &Service) -> ServiceRole {
    let is_robotlb = is_robotlb_load_balancer(svc);
    let owned = finalizers::check(svc);
    let deleting = svc.meta().deletion_timestamp.is_some();
    if (deleting && is_robotlb) || (owned && !is_robotlb) {
        ServiceRole::Release
    } else if is_robotlb && !deleting {
        ServiceRole::Reconcile
    } else {
        ServiceRole::Skip
    }
}

fn balancer_for(
    role: &ServiceRole,
    svc: &Service,
    context: &CurrentContext,
) -> RobotLBResult<LoadBalancer> {
    if *role == ServiceRole::Release {
        LoadBalancer::for_release(svc, context.hcloud_config.clone())
    } else {
        LoadBalancer::try_from_svc(svc, context)
    }
}

async fn sync_service(svc: Arc<Service>, context: Arc<CurrentContext>) -> RobotLBResult<Action> {
    let role = service_role(&svc);
    if role == ServiceRole::Skip {
        tracing::debug!("Service is not a robotlb load balancer. Skipping...");
        return Err(RobotLBError::SkipService);
    }

    // Hetzner counts requests per project, so while one service is rate limited
    // every other one waits too instead of spending the budget being waited for.
    if let Some(wait) = context.rate_limit.remaining(Instant::now()) {
        return Err(RobotLBError::RateLimited(wait));
    }

    tracing::info!("Starting service reconcilation");

    let lb = balancer_for(&role, &svc, &context)?;

    if role == ServiceRole::Release {
        tracing::info!("Service no longer needs a load balancer. Cleaning up resources.");
        lb.cleanup().await?;
        finalizers::remove(context.client.clone(), &svc).await?;
        return Ok(Action::await_change());
    }

    // Add finalizer if it's not there yet.
    if !finalizers::check(&svc) {
        finalizers::add(context.client.clone(), &svc).await?;
    }

    // Based on the service type, we will reconcile the load balancer.
    reconcile_load_balancer(lb, svc.clone(), context).await
}

/// Get the nodes kube-proxy serves the service's Local traffic from. Reading the
/// slices rather than the pods also covers services whose slices another controller
/// manages (e.g. kubevirt cloud-controller-manager).
async fn get_nodes_from_endpointslices(
    svc: &Arc<Service>,
    context: &Arc<CurrentContext>,
) -> RobotLBResult<Vec<Node>> {
    let namespace = svc
        .namespace()
        .unwrap_or_else(|| context.client.default_namespace().to_string());
    let eps_api = kube::Api::<EndpointSlice>::namespaced(context.client.clone(), &namespace);
    let eps_list = eps_api
        .list(&ListParams {
            label_selector: Some(format!("kubernetes.io/service-name={}", svc.name_any())),
            ..Default::default()
        })
        .await?;

    let target_nodes = eps_list
        .iter()
        .flat_map(triggers::slice_nodes)
        .collect::<HashSet<_>>();

    if target_nodes.is_empty() {
        tracing::warn!("No ready endpoints found in EndpointSlices for service");
        return Ok(vec![]);
    }

    tracing::info!(
        "Discovered {} target node(s) from EndpointSlices",
        target_nodes.len()
    );

    let nodes_api = kube::Api::<Node>::all(context.client.clone());
    let nodes = nodes_api
        .list(&ListParams::default())
        .await?
        .into_iter()
        .filter(|node| target_nodes.contains(&node.name_any()) && !is_excluded_from_lb(node))
        .collect::<Vec<_>>();

    Ok(nodes)
}

/// Get nodes based on the node selector.
/// This method will find the nodes based on the node selector
/// from the service annotations.
async fn get_nodes_by_selector(
    svc: &Arc<Service>,
    context: &Arc<CurrentContext>,
) -> RobotLBResult<Vec<Node>> {
    let node_selector = svc
        .annotations()
        .get(consts::LB_NODE_SELECTOR)
        .map(String::as_str)
        .ok_or(RobotLBError::ServiceWithoutSelector)?;
    let label_filter = LabelFilter::from_str(node_selector)?;
    let nodes_api = kube::Api::<Node>::all(context.client.clone());
    let nodes = nodes_api
        .list(&ListParams::default())
        .await?
        .into_iter()
        .filter(|node| label_filter.check(node.labels()) && !is_excluded_from_lb(node))
        .collect::<Vec<_>>();
    Ok(nodes)
}

/// Get every node of the cluster that may serve load balancer traffic.
async fn get_all_nodes(context: &Arc<CurrentContext>) -> RobotLBResult<Vec<Node>> {
    let nodes_api = kube::Api::<Node>::all(context.client.clone());
    let nodes = nodes_api
        .list(&ListParams::default())
        .await?
        .into_iter()
        .filter(is_lb_eligible_node)
        .collect::<Vec<_>>();
    Ok(nodes)
}

/// Whether the cluster declares that a node must stay out of external load balancers.
/// The label holds under every traffic policy, the way upstream cloud providers treat it.
fn is_excluded_from_lb(node: &Node) -> bool {
    node.labels()
        .contains_key(consts::EXCLUDE_FROM_LB_LABEL_NAME)
}

/// Whether a node may be used as a load balancer target under the `Cluster` policy,
/// where any node can carry the traffic and a draining or unhealthy one only takes
/// up a target slot.
fn is_lb_eligible_node(node: &Node) -> bool {
    if is_excluded_from_lb(node) {
        tracing::debug!("Node {} is excluded from load balancers", node.name_any());
        return false;
    }
    if node.spec.as_ref().and_then(|spec| spec.unschedulable) == Some(true) {
        tracing::debug!("Node {} is unschedulable", node.name_any());
        return false;
    }
    let ready = node
        .status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .and_then(|conditions| conditions.iter().find(|cond| cond.type_ == "Ready"))
        .map(|cond| cond.status.as_str());
    if matches!(ready, Some(status) if status != "True") {
        tracing::debug!("Node {} is not ready", node.name_any());
        return false;
    }
    true
}

/// Where the target nodes of a service come from.
#[derive(Debug, PartialEq, Eq)]
enum NodeSource {
    /// The `robotlb/node-selector` annotation of the service.
    Annotation,
    /// The nodes hosting the endpoints of the service.
    ServiceEndpoints,
    /// Every node that may serve load balancer traffic.
    AllNodes,
}

fn node_source(svc: &Service, dynamic_node_selector: bool) -> NodeSource {
    if !dynamic_node_selector {
        return NodeSource::Annotation;
    }
    if is_local_traffic_policy(svc) {
        return NodeSource::ServiceEndpoints;
    }
    NodeSource::AllNodes
}

/// Whether the service asks for traffic to reach only the nodes that host its endpoints.
/// Under the default `Cluster` policy every node is a valid target, because kube-proxy
/// forwards the traffic to a node that actually hosts a pod.
///
/// <https://kubernetes.io/docs/reference/networking/virtual-ips/#external-traffic-policy>
fn is_local_traffic_policy(svc: &Service) -> bool {
    svc.spec
        .as_ref()
        .and_then(|spec| spec.external_traffic_policy.as_deref())
        == Some("Local")
}

/// Map the ports of a service onto load balancer services.
fn collect_lb_services(svc: &Service) -> Vec<LBService> {
    let mut services = Vec::new();
    for port in svc.spec.iter().flat_map(|spec| spec.ports.iter().flatten()) {
        let protocol = port.protocol.as_deref().unwrap_or("TCP");
        if protocol != "TCP" {
            tracing::warn!("Protocol {} is not supported. Skipping...", protocol);
            continue;
        }
        let Some(node_port) = port.node_port else {
            tracing::warn!(
                "Service port {} has no nodePort allocated. Hetzner load balancers forward \
                 traffic to node IPs, so such a port cannot be exposed. Skipping...",
                port.port
            );
            continue;
        };
        services.push(LBService {
            listen_port: port.port,
            target_port: node_port,
        });
    }
    services
}

/// Reconcile the `LoadBalancer` type of service.
/// This function will find the nodes based on the node selector
/// and create or update the load balancer.
pub async fn reconcile_load_balancer(
    mut lb: LoadBalancer,
    svc: Arc<Service>,
    context: Arc<CurrentContext>,
) -> RobotLBResult<Action> {
    let node_ip_type = if lb.network_name.is_none() {
        "ExternalIP"
    } else {
        "InternalIP"
    };

    let nodes = match node_source(&svc, context.config.dynamic_node_selector) {
        NodeSource::Annotation => get_nodes_by_selector(&svc, &context).await?,
        NodeSource::ServiceEndpoints => get_nodes_from_endpointslices(&svc, &context).await?,
        NodeSource::AllNodes => get_all_nodes(&context).await?,
    };

    for node in nodes {
        let Some(status) = node.status else {
            continue;
        };
        let Some(addresses) = status.addresses else {
            continue;
        };
        for addr in addresses {
            if addr.type_ == node_ip_type {
                lb.add_target(&addr.address);
            }
        }
    }

    for service in collect_lb_services(&svc) {
        lb.add_service(service.listen_port, service.target_port);
    }

    // A balancer without a single service forwards nothing while still being billed,
    // so none is created until the service has a port that can be exposed.
    // An existing balancer and the address it gave the service are kept: a missing
    // nodePort is far more likely a mistake than a request to delete the balancer.
    if lb.services.is_empty() {
        return Err(RobotLBError::NoExposablePorts);
    }

    let (hcloud_lb, targets_missing) = lb.reconcile().await?;

    let svc_api = kube::Api::<Service>::namespaced(
        context.client.clone(),
        svc.namespace()
            .unwrap_or_else(|| context.client.default_namespace().to_string())
            .as_str(),
    );

    let mut ingress = vec![];

    let dns_ipv4 = hcloud_lb.public_net.ipv4.dns_ptr.flatten();
    let ipv4 = hcloud_lb.public_net.ipv4.ip.flatten();
    let dns_ipv6 = hcloud_lb.public_net.ipv6.dns_ptr.flatten();
    let ipv6 = hcloud_lb.public_net.ipv6.ip.flatten();
    if let Some(ipv4) = &ipv4 {
        ingress.push(json!({
            "ip": ipv4,
            "dns": dns_ipv4,
            "ip_mode": "VIP"
        }));
    }
    if context.config.ipv6_ingress {
        if let Some(ipv6) = &ipv6 {
            ingress.push(json!({
                "ip": ipv6,
                "dns": dns_ipv6,
                "ip_mode": "VIP"
            }));
        }
    }

    if !ingress.is_empty() {
        svc_api
            .patch_status(
                svc.name_any().as_str(),
                &PatchParams::default(),
                &kube::api::Patch::Merge(json!({
                    "status" :{
                        "loadBalancer": {
                            "ingress": ingress
                        }
                    }
                })),
            )
            .await?;
    }

    Ok(Action::requeue(success_requeue(
        targets_missing,
        context.config.resync_interval,
    )))
}

/// A target Hetzner rejected, for a reason other than the rate limit, may be accepted
/// on the next try, so it is retried as soon as a failed reconcile would be.
const fn success_requeue(targets_missing: bool, resync_interval: u64) -> Duration {
    if targets_missing && resync_interval > 30 {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(resync_interval)
    }
}

/// Handle the error during reconcilation.
#[allow(clippy::needless_pass_by_value)]
fn on_error(svc: Arc<Service>, error: &RobotLBError, context: Arc<CurrentContext>) -> Action {
    error_action(
        error,
        &context.rate_limit,
        &context.error_backoff,
        Instant::now(),
        &service_key(&svc),
    )
}

fn error_action(
    error: &RobotLBError,
    rate_limit: &RateLimitGate,
    backoff: &ErrorBackoff,
    now: Instant,
    service: &str,
) -> Action {
    match error {
        RobotLBError::SkipService => Action::await_change(),
        RobotLBError::RateLimited(wait) => Action::requeue(spread(*wait, service)),
        error if error.is_rate_limited() => {
            Action::requeue(spread(rate_limit.on_rate_limited(now), service))
        }
        _ => Action::requeue(backoff.on_failure(service, now)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        balancer_for, collect_lb_services, consts, error_action, event_note, is_excluded_from_lb,
        is_lb_eligible_node, is_local_traffic_policy, node_source, publishes_event, service_role,
        slice_triggers, success_requeue, CurrentContext, HCloudConfig, NodeSource, OperatorConfig,
        RobotLBError, ServiceRole,
    };
    use clap::Parser;
    use k8s_openapi::{
        api::core::v1::{
            Node, NodeCondition, NodeSpec, NodeStatus, Service, ServicePort, ServiceSpec,
        },
        apimachinery::pkg::apis::meta::v1::ObjectMeta,
    };
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn only_a_reconciliation_reads_the_annotations() {
        let config = OperatorConfig::try_parse_from(["robotlb", "--hcloud-token", "t"]).unwrap();
        let client =
            kube::Client::try_from(kube::Config::new("http://127.0.0.1:1".parse().unwrap()))
                .unwrap();
        let context = CurrentContext::new(client, config, HCloudConfig::default());
        let svc = Service {
            metadata: ObjectMeta {
                uid: Some("uid-1".to_string()),
                annotations: Some(
                    [(consts::LB_RETRIES_ANN_NAME.to_string(), "abc".to_string())].into(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(balancer_for(&ServiceRole::Release, &svc, &context).is_ok());
        assert!(balancer_for(&ServiceRole::Reconcile, &svc, &context).is_err());
        assert!(matches!(
            balancer_for(&ServiceRole::Release, &Service::default(), &context),
            Err(RobotLBError::SkipService)
        ));
    }

    fn service(spec: ServiceSpec) -> Service {
        Service {
            spec: Some(spec),
            ..Default::default()
        }
    }

    fn node(labels: &[(&str, &str)], unschedulable: bool) -> Node {
        Node {
            metadata: ObjectMeta {
                labels: Some(
                    labels
                        .iter()
                        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                        .collect::<BTreeMap<_, _>>(),
                ),
                ..Default::default()
            },
            spec: Some(NodeSpec {
                unschedulable: Some(unschedulable),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn node_with_ready_condition(status: &str) -> Node {
        Node {
            status: Some(NodeStatus {
                conditions: Some(vec![NodeCondition {
                    type_: "Ready".to_string(),
                    status: status.to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..node(&[], false)
        }
    }

    #[test]
    fn cluster_policy_is_not_local() {
        let svc = service(ServiceSpec {
            external_traffic_policy: Some("Cluster".into()),
            ..Default::default()
        });
        assert!(!is_local_traffic_policy(&svc));
    }

    #[test]
    fn unset_policy_defaults_to_cluster() {
        assert!(!is_local_traffic_policy(&service(ServiceSpec::default())));
    }

    #[test]
    fn local_policy_is_local() {
        let svc = service(ServiceSpec {
            external_traffic_policy: Some("Local".into()),
            ..Default::default()
        });
        assert!(is_local_traffic_policy(&svc));
    }

    #[test]
    fn ports_map_listen_port_to_node_port() {
        let svc = service(ServiceSpec {
            ports: Some(vec![ServicePort {
                port: 22,
                node_port: Some(30821),
                protocol: Some("TCP".into()),
                ..Default::default()
            }]),
            ..Default::default()
        });
        let services = collect_lb_services(&svc);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].listen_port, 22);
        assert_eq!(services[0].target_port, 30821);
    }

    #[test]
    fn ports_without_protocol_are_treated_as_tcp() {
        let svc = service(ServiceSpec {
            ports: Some(vec![ServicePort {
                port: 80,
                node_port: Some(31571),
                ..Default::default()
            }]),
            ..Default::default()
        });
        let services = collect_lb_services(&svc);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].listen_port, 80);
        assert_eq!(services[0].target_port, 31571);
    }

    #[test]
    fn udp_ports_are_skipped() {
        let svc = service(ServiceSpec {
            ports: Some(vec![ServicePort {
                port: 53,
                node_port: Some(30053),
                protocol: Some("UDP".into()),
                ..Default::default()
            }]),
            ..Default::default()
        });
        assert!(collect_lb_services(&svc).is_empty());
    }

    #[test]
    fn ports_without_node_port_are_skipped() {
        let svc = service(ServiceSpec {
            ports: Some(vec![ServicePort {
                port: 22,
                protocol: Some("TCP".into()),
                ..Default::default()
            }]),
            ..Default::default()
        });
        assert!(collect_lb_services(&svc).is_empty());
    }

    #[test]
    fn plain_node_is_eligible() {
        assert!(is_lb_eligible_node(&node(
            &[("kubernetes.io/hostname", "ws1")],
            false
        )));
    }

    #[test]
    fn excluded_node_is_not_eligible() {
        assert!(!is_lb_eligible_node(&node(
            &[(consts::EXCLUDE_FROM_LB_LABEL_NAME, "")],
            false
        )));
    }

    #[test]
    fn cordoned_node_is_not_eligible() {
        assert!(!is_lb_eligible_node(&node(&[], true)));
    }

    #[test]
    fn ready_node_is_eligible() {
        assert!(is_lb_eligible_node(&node_with_ready_condition("True")));
    }

    #[test]
    fn not_ready_node_is_not_eligible() {
        assert!(!is_lb_eligible_node(&node_with_ready_condition("False")));
    }

    #[test]
    fn exclusion_label_is_recognised_on_its_own() {
        assert!(is_excluded_from_lb(&node(
            &[(consts::EXCLUDE_FROM_LB_LABEL_NAME, "")],
            false
        )));
        assert!(!is_excluded_from_lb(&node(&[], true)));
    }

    #[test]
    fn cluster_policy_takes_every_node() {
        let svc = service(ServiceSpec::default());
        assert_eq!(node_source(&svc, true), NodeSource::AllNodes);
    }

    #[test]
    fn local_policy_takes_endpoint_nodes() {
        let svc = service(ServiceSpec {
            external_traffic_policy: Some("Local".into()),
            ..Default::default()
        });
        assert_eq!(node_source(&svc, true), NodeSource::ServiceEndpoints);
    }

    #[test]
    fn static_selector_wins_over_the_policy() {
        let svc = service(ServiceSpec {
            external_traffic_policy: Some("Local".into()),
            ..Default::default()
        });
        assert_eq!(node_source(&svc, false), NodeSource::Annotation);
    }

    #[test]
    fn an_event_note_is_redacted_and_bounded() {
        let token = "0123456789abcdef";
        let error = crate::error::RobotLBError::HCloudError(format!(
            "rejected token {} {}",
            &token[..10],
            "é".repeat(600)
        ));
        let note = event_note(&error, token);
        assert!(!note.contains(&token[..10]));
        assert!(note.contains("[REDACTED]"));
        assert!(note.len() <= 1024);
    }

    #[test]
    fn a_gated_service_waits_out_the_pause() {
        let gate = crate::rate_limit::RateLimitGate::default();
        let wait = std::time::Duration::from_secs(42);
        let error = crate::error::RobotLBError::RateLimited(wait);
        assert_eq!(
            error_action(
                &error,
                &gate,
                &crate::backoff::ErrorBackoff::default(),
                std::time::Instant::now(),
                "shop/web"
            ),
            kube::runtime::controller::Action::requeue(crate::rate_limit::spread(wait, "shop/web"))
        );
    }

    #[test]
    fn a_rate_limited_call_closes_the_gate() {
        let gate = crate::rate_limit::RateLimitGate::default();
        let now = std::time::Instant::now();
        let error = crate::error::RobotLBError::from(hcloud::apis::Error::<
            hcloud::apis::load_balancers_api::AddTargetError,
        >::ResponseError(
            hcloud::apis::ResponseContent {
                status: 429_u16.try_into().unwrap(),
                content: String::new(),
                entity: None,
            },
        ));
        assert_eq!(
            error_action(
                &error,
                &gate,
                &crate::backoff::ErrorBackoff::default(),
                now,
                "shop/web"
            ),
            kube::runtime::controller::Action::requeue(crate::rate_limit::spread(
                std::time::Duration::from_secs(60),
                "shop/web"
            ))
        );
        assert!(gate.remaining(now).is_some());
    }

    #[test]
    fn other_errors_back_off_per_service() {
        let gate = crate::rate_limit::RateLimitGate::default();
        let backoff = crate::backoff::ErrorBackoff::default();
        let now = std::time::Instant::now();
        let error = crate::error::RobotLBError::HCloudError("boom".to_string());
        let retry = |service| error_action(&error, &gate, &backoff, now, service);
        let after =
            |secs| kube::runtime::controller::Action::requeue(std::time::Duration::from_secs(secs));
        assert_eq!(retry("shop/web"), after(5));
        assert_eq!(retry("shop/web"), after(10));
        assert_eq!(retry("shop/api"), after(5));
    }

    #[test]
    fn rate_limits_do_not_lengthen_the_error_backoff() {
        let gate = crate::rate_limit::RateLimitGate::default();
        let backoff = crate::backoff::ErrorBackoff::default();
        let now = std::time::Instant::now();
        let gated = crate::error::RobotLBError::RateLimited(std::time::Duration::from_secs(42));
        error_action(&gated, &gate, &backoff, now, "shop/web");
        let rejected = crate::error::RobotLBError::from(hcloud::apis::Error::<
            hcloud::apis::load_balancers_api::AddTargetError,
        >::ResponseError(
            hcloud::apis::ResponseContent {
                status: 429_u16.try_into().unwrap(),
                content: String::new(),
                entity: None,
            },
        ));
        error_action(&rejected, &gate, &backoff, now, "shop/web");
        let error = crate::error::RobotLBError::HCloudError("boom".to_string());
        assert_eq!(
            error_action(&error, &gate, &backoff, now, "shop/web"),
            kube::runtime::controller::Action::requeue(std::time::Duration::from_secs(5))
        );
    }

    #[test]
    fn success_and_skips_end_the_backoff_but_a_closed_gate_does_not() {
        use crate::error::RobotLBError;
        use kube::runtime::controller::Action;
        assert!(super::ends_backoff(&Ok(Action::await_change())));
        assert!(super::ends_backoff(&Err(RobotLBError::SkipService)));
        assert!(!super::ends_backoff(&Err(RobotLBError::RateLimited(
            std::time::Duration::from_secs(1)
        ))));
        assert!(!super::ends_backoff(&Err(RobotLBError::HCloudError(
            "boom".to_string()
        ))));
    }

    #[test]
    fn every_failure_except_a_skip_publishes_an_event() {
        use crate::error::RobotLBError;
        assert!(!publishes_event(&RobotLBError::SkipService));
        // Jitter wakes the same service first after every pause, so the others only
        // ever see the closed gate, and without an event of their own they go silent.
        assert!(publishes_event(&RobotLBError::RateLimited(
            std::time::Duration::from_secs(1)
        )));
        assert!(publishes_event(&RobotLBError::from(hcloud::apis::Error::<
            hcloud::apis::load_balancers_api::ListLoadBalancersError,
        >::ResponseError(
            hcloud::apis::ResponseContent {
                status: 429_u16.try_into().unwrap(),
                content: String::new(),
                entity: None,
            }
        ))));
        assert!(publishes_event(&RobotLBError::HCloudError(
            "boom".to_string()
        )));
        assert!(publishes_event(&RobotLBError::UnrecognisedBalancer {
            name: "web".to_string(),
            uid: "uid-1".to_string(),
        }));
        assert!(publishes_event(&RobotLBError::ForeignBalancer {
            name: "web".to_string(),
            owner: "uid-2".to_string(),
        }));
        assert!(publishes_event(&RobotLBError::NoNodesToRecogniseBalancer(
            "web".to_string()
        )));
        assert!(publishes_event(&RobotLBError::AmbiguousBalancer(
            "web".to_string()
        )));
    }

    fn owned_service(type_: &str, class: Option<&str>, deleting: bool) -> Service {
        Service {
            metadata: ObjectMeta {
                finalizers: Some(vec![consts::FINALIZER_NAME.to_string()]),
                deletion_timestamp: deleting.then(|| {
                    k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
                        k8s_openapi::chrono::Utc::now(),
                    )
                }),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                type_: Some(type_.to_string()),
                load_balancer_class: class.map(str::to_string),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_robotlb_load_balancer_is_reconciled() {
        let svc = service(ServiceSpec {
            type_: Some("LoadBalancer".to_string()),
            ..Default::default()
        });
        assert_eq!(service_role(&svc), ServiceRole::Reconcile);
        let svc = owned_service("LoadBalancer", Some(consts::ROBOTLB_LB_CLASS), false);
        assert_eq!(service_role(&svc), ServiceRole::Reconcile);
    }

    #[test]
    fn services_robotlb_does_not_own_are_skipped() {
        let other_class = service(ServiceSpec {
            type_: Some("LoadBalancer".to_string()),
            load_balancer_class: Some("example.com/other".to_string()),
            ..Default::default()
        });
        assert_eq!(service_role(&other_class), ServiceRole::Skip);
        let cluster_ip = service(ServiceSpec {
            type_: Some("ClusterIP".to_string()),
            ..Default::default()
        });
        assert_eq!(service_role(&cluster_ip), ServiceRole::Skip);
        let mut deleting_cluster_ip = owned_service("ClusterIP", None, true);
        deleting_cluster_ip.metadata.finalizers = None;
        assert_eq!(service_role(&deleting_cluster_ip), ServiceRole::Skip);
        let mut deleting_other_class =
            owned_service("LoadBalancer", Some("example.com/other"), true);
        deleting_other_class.metadata.finalizers = None;
        assert_eq!(service_role(&deleting_other_class), ServiceRole::Skip);
    }

    // A type change and a change back with another class can both land before robotlb
    // reconciles, for example while the rate limit gate is closed.
    #[test]
    fn an_owned_service_taken_over_by_another_class_is_released() {
        let svc = owned_service("LoadBalancer", Some("example.com/other"), false);
        assert_eq!(service_role(&svc), ServiceRole::Release);
    }

    // Reachable when the finalizer was removed by hand while another controller's
    // finalizer still holds the object.
    #[test]
    fn a_deleted_robotlb_load_balancer_without_the_finalizer_is_released() {
        let mut svc = owned_service("LoadBalancer", None, true);
        svc.metadata.finalizers = None;
        assert_eq!(service_role(&svc), ServiceRole::Release);
    }

    #[test]
    fn an_owned_service_that_stopped_being_a_load_balancer_is_released() {
        assert_eq!(
            service_role(&owned_service("ClusterIP", None, false)),
            ServiceRole::Release
        );
    }

    #[test]
    fn a_deleted_owned_service_is_released() {
        assert_eq!(
            service_role(&owned_service("LoadBalancer", None, true)),
            ServiceRole::Release
        );
        assert_eq!(
            service_role(&owned_service("ClusterIP", None, true)),
            ServiceRole::Release
        );
    }

    #[test]
    fn a_service_without_exposable_ports_reports_an_event() {
        let error = crate::error::RobotLBError::NoExposablePorts;
        assert!(publishes_event(&error));
        assert!(error.to_string().starts_with("No TCP port"));
    }

    fn endpoint_slice(node: &str) -> k8s_openapi::api::discovery::v1::EndpointSlice {
        k8s_openapi::api::discovery::v1::EndpointSlice {
            metadata: ObjectMeta {
                name: Some("web-abcde".to_string()),
                namespace: Some("shop".to_string()),
                ..Default::default()
            },
            endpoints: vec![k8s_openapi::api::discovery::v1::Endpoint {
                node_name: Some(node.to_string()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn live(
        slice: &k8s_openapi::api::discovery::v1::EndpointSlice,
    ) -> crate::triggers::SliceChanges {
        use kube::runtime::watcher::Event;
        let changes = crate::triggers::SliceChanges::default();
        changes.track(&Event::<k8s_openapi::api::discovery::v1::EndpointSlice>::Init);
        changes.track(&Event::InitApply(slice.clone()));
        changes.track(&Event::<k8s_openapi::api::discovery::v1::EndpointSlice>::InitDone);
        changes
    }

    fn with_policy(policy: &str) -> Service {
        service(ServiceSpec {
            type_: Some("LoadBalancer".to_string()),
            external_traffic_policy: Some(policy.to_string()),
            ..Default::default()
        })
    }

    #[test]
    fn a_slice_of_a_local_service_triggers_only_when_its_nodes_change() {
        let slice = endpoint_slice("a");
        let changes = live(&slice);
        let svc = with_policy("Local");
        assert!(slice_triggers(&slice, Some(&svc), true, &changes));
        assert!(!slice_triggers(&slice, Some(&svc), true, &changes));
        assert!(slice_triggers(
            &endpoint_slice("b"),
            Some(&svc),
            true,
            &changes
        ));
    }

    #[test]
    fn a_slice_of_a_service_that_ignores_endpoints_triggers_nothing_and_is_forgotten() {
        let slice = endpoint_slice("a");
        let changes = live(&slice);
        let local = with_policy("Local");
        slice_triggers(&slice, Some(&local), true, &changes);
        assert!(!slice_triggers(
            &slice,
            Some(&with_policy("Cluster")),
            true,
            &changes
        ));
        assert!(!slice_triggers(&slice, Some(&local), false, &changes));
        assert!(slice_triggers(&slice, Some(&local), true, &changes));
    }

    #[test]
    fn a_slice_without_a_known_service_triggers_nothing() {
        let slice = endpoint_slice("a");
        assert!(!slice_triggers(&slice, None, true, &live(&slice)));
    }

    fn flapping_slice(ready: bool) -> k8s_openapi::api::discovery::v1::EndpointSlice {
        let mut slice = endpoint_slice("a");
        slice.endpoints[0].conditions = Some(k8s_openapi::api::discovery::v1::EndpointConditions {
            ready: Some(ready),
            ..Default::default()
        });
        slice
    }

    // A cordoned or not-ready node keeps running its pods, so it is the readiness of
    // their endpoints that takes the node out of the targets.
    #[test]
    fn a_readiness_flap_triggers_a_local_service_with_a_selector() {
        let with_selector = service(ServiceSpec {
            type_: Some("LoadBalancer".to_string()),
            external_traffic_policy: Some("Local".to_string()),
            selector: Some(BTreeMap::from([("app".to_string(), "web".to_string())])),
            ..Default::default()
        });
        let ready = flapping_slice(true);
        let changes = live(&ready);
        slice_triggers(&ready, Some(&with_selector), true, &changes);
        assert!(slice_triggers(
            &flapping_slice(false),
            Some(&with_selector),
            true,
            &changes
        ));
    }

    // Deleting a slice with recorded nodes reconciles every balancer, which a service
    // without a robotlb balancer must not cost.
    #[test]
    fn slices_of_services_without_a_robotlb_balancer_are_not_observed() {
        let slice = endpoint_slice("a");
        for spec in [
            ServiceSpec {
                type_: Some("NodePort".to_string()),
                external_traffic_policy: Some("Local".to_string()),
                ..Default::default()
            },
            ServiceSpec {
                type_: Some("LoadBalancer".to_string()),
                load_balancer_class: Some("example.com/other".to_string()),
                external_traffic_policy: Some("Local".to_string()),
                ..Default::default()
            },
        ] {
            let changes = live(&slice);
            assert!(!slice_triggers(
                &slice,
                Some(&service(spec)),
                true,
                &changes
            ));
            assert!(!changes.track(&kube::runtime::watcher::Event::Delete(slice.clone())));
        }
    }

    // A node whose target could not be added would otherwise wait for the resync,
    // since a partly successful reconcile still counts as a success.
    #[test]
    fn missing_targets_are_retried_like_an_error() {
        use std::time::Duration;
        assert_eq!(success_requeue(true, 300), Duration::from_secs(30));
        assert_eq!(success_requeue(false, 300), Duration::from_secs(300));
        assert_eq!(success_requeue(true, 10), Duration::from_secs(10));
    }
}
