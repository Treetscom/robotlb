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

use clap::Parser;
use config::OperatorConfig;
use error::{RobotLBError, RobotLBResult};
use futures::StreamExt;
use hcloud::apis::configuration::Configuration as HCloudConfig;
use k8s_openapi::{
    api::{
        core::v1::{Node, Pod, Service},
        discovery::v1::EndpointSlice,
    },
    serde_json::json,
};
use kube::{
    api::{ListParams, PatchParams},
    runtime::{controller::Action, watcher, Controller},
    Resource, ResourceExt,
};
use label_filter::LabelFilter;
use lb::{LBService, LoadBalancer};
use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};

pub mod config;
pub mod consts;
pub mod error;
pub mod finalizers;
pub mod label_filter;
pub mod lb;

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
    watcher::Config::default();
    let context = Arc::new(CurrentContext::new(
        kube_client.clone(),
        operator_config.clone(),
        hcloud_conf,
    ));
    tracing::info!("Starting the controller");
    Controller::new(
        kube::Api::<Service>::all(kube_client),
        watcher::Config::default(),
    )
    .run(reconcile_service, on_error, context)
    .for_each(|reconcilation_result| async move {
        match reconcilation_result {
            Ok((service, _action)) => {
                tracing::info!("Reconcilation of a service {} was successful", service.name);
            }
            Err(err) => match err {
                // During reconcilation process,
                // the controller has decided to skip the service.
                kube::runtime::controller::Error::ReconcilerFailed(
                    RobotLBError::SkipService,
                    _,
                ) => {}
                _ => {
                    tracing::error!("Error reconciling service: {:#?}", err);
                }
            },
        }
    })
    .await;
    Ok(())
}

#[derive(Clone)]
pub struct CurrentContext {
    pub client: kube::Client,
    pub config: OperatorConfig,
    pub hcloud_config: HCloudConfig,
}
impl CurrentContext {
    #[must_use]
    pub const fn new(
        client: kube::Client,
        config: OperatorConfig,
        hcloud_config: HCloudConfig,
    ) -> Self {
        Self {
            client,
            config,
            hcloud_config,
        }
    }
}

/// Reconcile the service.
/// This function is called by the controller for each service.
/// It will create or update the load balancer based on the service.
/// If the service is being deleted, it will clean up the resources.
#[tracing::instrument(skip(svc,context), fields(service=svc.name_any()))]
pub async fn reconcile_service(
    svc: Arc<Service>,
    context: Arc<CurrentContext>,
) -> RobotLBResult<Action> {
    let svc_type = svc
        .spec
        .as_ref()
        .and_then(|s| s.type_.as_ref())
        .map(String::as_str)
        .unwrap_or("ClusterIP");
    if svc_type != "LoadBalancer" {
        tracing::debug!("Service type is not LoadBalancer. Skipping...");
        return Err(RobotLBError::SkipService);
    }

    let lb_type = svc
        .spec
        .as_ref()
        .and_then(|s| s.load_balancer_class.as_ref())
        .map(String::as_str)
        .unwrap_or(consts::ROBOTLB_LB_CLASS);
    if lb_type != consts::ROBOTLB_LB_CLASS {
        tracing::debug!("Load balancer class is not robotlb. Skipping...");
        return Err(RobotLBError::SkipService);
    }

    tracing::info!("Starting service reconcilation");

    let lb = LoadBalancer::try_from_svc(&svc, &context)?;

    // If the service is being deleted, we need to clean up the resources.
    if svc.meta().deletion_timestamp.is_some() {
        tracing::info!("Service deletion detected. Cleaning up resources.");
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

/// Method to get nodes dynamically based on the pods.
/// This method will find the nodes where the target pods are deployed.
/// It will use the pod selector to find the pods and then get the nodes.
async fn get_nodes_dynamically(
    svc: &Arc<Service>,
    context: &Arc<CurrentContext>,
) -> RobotLBResult<Vec<Node>> {
    let pod_api = kube::Api::<Pod>::namespaced(
        context.client.clone(),
        svc.namespace()
            .as_ref()
            .map(String::as_str)
            .unwrap_or_else(|| context.client.default_namespace()),
    );

    let Some(pod_selector) = svc
        .spec
        .as_ref()
        .and_then(|spec| spec.selector.clone())
        .filter(|s| !s.is_empty())
    else {
        tracing::info!(
            "Service has no selector, falling back to EndpointSlice-based node discovery"
        );
        return get_nodes_from_endpointslices(svc, context).await;
    };

    let label_selector = pod_selector
        .iter()
        .map(|(key, val)| format!("{key}={val}"))
        .collect::<Vec<_>>()
        .join(",");

    let pods = pod_api
        .list(&ListParams {
            label_selector: Some(label_selector),
            ..Default::default()
        })
        .await?;

    let target_nodes = pods
        .iter()
        .map(|pod| pod.spec.clone().unwrap_or_default().node_name)
        .flatten()
        .collect::<HashSet<_>>();

    let nodes_api = kube::Api::<Node>::all(context.client.clone());
    let nodes = nodes_api
        .list(&ListParams::default())
        .await?
        .into_iter()
        .filter(|node| target_nodes.contains(&node.name_any()))
        .collect::<Vec<_>>();

    Ok(nodes)
}

/// Get nodes from `EndpointSlice` resources associated with a Service.
/// This method is used as a fallback when the Service has no selector,
/// such as when `EndpointSlice` resources are managed by an external controller
/// (e.g. kubevirt cloud-controller-manager).
/// It discovers target nodes by reading the `nodeName` field from each endpoint.
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
        .into_iter()
        .flat_map(|eps| eps.endpoints)
        .filter(|ep| ep.conditions.as_ref().and_then(|c| c.ready).unwrap_or(true))
        .filter_map(|ep| ep.node_name)
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
        .filter(|node| target_nodes.contains(&node.name_any()))
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
        .filter(|node| label_filter.check(node.labels()))
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

/// Whether a node may be used as a load balancer target.
/// Cordoned nodes are draining, and the exclusion label is how a cluster declares
/// that a node must stay out of external load balancers.
fn is_lb_eligible_node(node: &Node) -> bool {
    if node
        .labels()
        .contains_key(consts::EXCLUDE_FROM_LB_LABEL_NAME)
    {
        tracing::debug!("Node {} is excluded from load balancers", node.name_any());
        return false;
    }
    if node.spec.as_ref().and_then(|spec| spec.unschedulable) == Some(true) {
        tracing::debug!("Node {} is unschedulable", node.name_any());
        return false;
    }
    true
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
    let mut node_ip_type = "InternalIP";
    if lb.network_name.is_none() {
        node_ip_type = "ExternalIP";
    }

    let nodes = if context.config.dynamic_node_selector {
        if is_local_traffic_policy(&svc) {
            get_nodes_dynamically(&svc, &context).await?
        } else {
            get_all_nodes(&context).await?
        }
    } else {
        get_nodes_by_selector(&svc, &context).await?
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

    if lb.services.is_empty() {
        tracing::warn!(
            "Service has no port that can be exposed. Leaving it without an external IP."
        );
    }

    let svc_api = kube::Api::<Service>::namespaced(
        context.client.clone(),
        svc.namespace()
            .unwrap_or_else(|| context.client.default_namespace().to_string())
            .as_str(),
    );

    let hcloud_lb = lb.reconcile().await?;

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
        }))
    }
    if context.config.ipv6_ingress {
        if let Some(ipv6) = &ipv6 {
            ingress.push(json!({
                "ip": ipv6,
                "dns": dns_ipv6,
                "ip_mode": "VIP"
            }))
        }
    }

    if !ingress.is_empty() && !lb.services.is_empty() {
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

    Ok(Action::requeue(Duration::from_secs(30)))
}

/// Handle the error during reconcilation.
#[allow(clippy::needless_pass_by_value)]
fn on_error(_: Arc<Service>, error: &RobotLBError, _context: Arc<CurrentContext>) -> Action {
    match error {
        RobotLBError::SkipService => Action::await_change(),
        _ => Action::requeue(Duration::from_secs(30)),
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_lb_services, consts, is_lb_eligible_node, is_local_traffic_policy};
    use k8s_openapi::{
        api::core::v1::{Node, NodeSpec, Service, ServicePort, ServiceSpec},
        apimachinery::pkg::apis::meta::v1::ObjectMeta,
    };
    use std::collections::BTreeMap;

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
}
