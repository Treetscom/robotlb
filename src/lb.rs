use hcloud::{
    apis::{
        configuration::Configuration as HcloudConfig,
        load_balancers_api::{
            AddServiceParams, AddTargetParams, AttachLoadBalancerToNetworkParams,
            ChangeAlgorithmParams, ChangeTypeOfLoadBalancerParams, DeleteLoadBalancerParams,
            DeleteServiceParams, DetachLoadBalancerFromNetworkParams, ListLoadBalancersParams,
            RemoveTargetParams, UpdateServiceParams,
        },
        networks_api::ListNetworksParams,
    },
    models::{
        AttachLoadBalancerToNetworkRequest, ChangeTypeOfLoadBalancerRequest, DeleteServiceRequest,
        DetachLoadBalancerFromNetworkRequest, LoadBalancerAddTarget, LoadBalancerAlgorithm,
        LoadBalancerService, LoadBalancerServiceHealthCheck, RemoveTargetRequest,
        UpdateLoadBalancerService,
    },
};
use k8s_openapi::api::core::v1::Service;
use kube::ResourceExt;
use std::{collections::HashMap, str::FromStr};

use crate::{
    consts,
    error::{RobotLBError, RobotLBResult},
    CurrentContext,
};

#[derive(Debug)]
pub struct LBService {
    pub listen_port: i32,
    pub target_port: i32,
}

enum LBAlgorithm {
    RoundRobin,
    LeastConnections,
}

/// Struct representing a load balancer
/// It holds all the necessary information to manage the load balancer
/// in Hetzner Cloud.
#[derive(Debug)]
pub struct LoadBalancer {
    pub name: String,
    pub services: HashMap<i32, i32>,
    pub targets: Vec<String>,
    pub private_ip: Option<String>,

    pub check_interval: i32,
    pub timeout: i32,
    pub retries: i32,
    pub proxy_mode: bool,

    pub location: String,
    pub balancer_type: String,
    pub algorithm: LoadBalancerAlgorithm,
    pub network_name: Option<String>,

    pub hcloud_config: HcloudConfig,
}

impl LoadBalancer {
    /// Create a new `LoadBalancer` instance from a Kubernetes service
    /// and the current context.
    /// This method will try to extract all the necessary information
    /// from the service annotations and the context.
    /// If some of the required information is missing, the method will
    /// try to use the default values from the context.
    pub fn try_from_svc(svc: &Service, context: &CurrentContext) -> RobotLBResult<Self> {
        let retries = svc
            .annotations()
            .get(consts::LB_RETRIES_ANN_NAME)
            .map(String::as_str)
            .map(i32::from_str)
            .transpose()?
            .unwrap_or(context.config.default_lb_retries);

        let timeout = svc
            .annotations()
            .get(consts::LB_TIMEOUT_ANN_NAME)
            .map(String::as_str)
            .map(i32::from_str)
            .transpose()?
            .unwrap_or(context.config.default_lb_timeout);

        let check_interval = svc
            .annotations()
            .get(consts::LB_CHECK_INTERVAL_ANN_NAME)
            .map(String::as_str)
            .map(i32::from_str)
            .transpose()?
            .unwrap_or(context.config.default_lb_interval);

        let proxy_mode = svc
            .annotations()
            .get(consts::LB_PROXY_MODE_LABEL_NAME)
            .map(String::as_str)
            .map(bool::from_str)
            .transpose()?
            .unwrap_or(context.config.default_lb_proxy_mode_enabled);

        let location = svc
            .annotations()
            .get(consts::LB_LOCATION_LABEL_NAME)
            .cloned()
            .unwrap_or_else(|| context.config.default_lb_location.clone());

        let balancer_type = svc
            .annotations()
            .get(consts::LB_BALANCER_TYPE_LABEL_NAME)
            .cloned()
            .unwrap_or_else(|| context.config.default_balancer_type.clone());

        let algorithm = svc
            .annotations()
            .get(consts::LB_ALGORITHM_LABEL_NAME)
            .map(String::as_str)
            .or(Some(&context.config.default_lb_algorithm))
            .map(LBAlgorithm::from_str)
            .transpose()?
            .unwrap_or(LBAlgorithm::LeastConnections);

        let network_name = svc
            .annotations()
            .get(consts::LB_NETWORK_LABEL_NAME)
            .or(context.config.default_network.as_ref())
            .cloned();

        let name = svc
            .annotations()
            .get(consts::LB_NAME_LABEL_NAME)
            .cloned()
            .unwrap_or(svc.name_any());

        let private_ip = svc
            .annotations()
            .get(consts::LB_PRIVATE_IP_LABEL_NAME)
            .cloned();

        Ok(Self {
            name,
            private_ip,
            balancer_type,
            check_interval,
            timeout,
            retries,
            location,
            proxy_mode,
            network_name,
            algorithm: algorithm.into(),
            services: HashMap::default(),
            targets: Vec::default(),
            hcloud_config: context.hcloud_config.clone(),
        })
    }

    /// Add a service to the load balancer.
    /// The service will listen on the `listen_port` and forward the
    /// traffic to the `target_port` to all targets.
    pub fn add_service(&mut self, listen_port: i32, target_port: i32) {
        self.services.insert(listen_port, target_port);
    }

    /// Add a target to the load balancer.
    /// The target will receive the traffic from the services.
    /// The target is identified by its IP address.
    pub fn add_target(&mut self, ip: &str) {
        tracing::debug!("Adding target {}", ip);
        self.targets.push(ip.to_string());
    }

    /// Reconcile the load balancer to match the desired configuration.
    #[tracing::instrument(skip(self), fields(lb_name=self.name))]
    pub async fn reconcile(&self) -> RobotLBResult<hcloud::models::LoadBalancer> {
        let hcloud_balancer = self.get_or_create_hcloud_lb().await?;
        self.reconcile_algorithm(&hcloud_balancer).await?;
        self.reconcile_lb_type(&hcloud_balancer).await?;
        self.reconcile_network(&hcloud_balancer).await?;
        self.reconcile_services(&hcloud_balancer).await?;
        self.reconcile_targets(&hcloud_balancer).await?;
        Ok(hcloud_balancer)
    }

    /// Reconcile the services of the load balancer.
    /// This method will compare the desired configuration of the services
    /// with the current configuration of the services in the load balancer.
    /// If the configuration does not match, the method will update the service.
    async fn reconcile_services(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<()> {
        for service in &hcloud_balancer.services {
            // Here we check that all the services are configured correctly.
            // If the service is not configured correctly, we update it.
            if let Some(destination_port) = self.services.get(&service.listen_port) {
                if service.destination_port == *destination_port
                    && service.health_check.port == *destination_port
                    && service.health_check.interval == self.check_interval
                    && service.health_check.retries == self.retries
                    && service.health_check.timeout == self.timeout
                    && service.proxyprotocol == self.proxy_mode
                    && service.http.is_none()
                    && service.health_check.protocol
                        == hcloud::models::load_balancer_service_health_check::Protocol::Tcp
                {
                    // The desired configuration matches the current configuration.
                    continue;
                }
                tracing::info!(
                    "Desired service configuration for port {} does not match current configuration. Updating ...",
                    service.listen_port,
                );
                hcloud::apis::load_balancers_api::update_service(
                        &self.hcloud_config,
                    UpdateServiceParams {
                        id: hcloud_balancer.id,
                        body: Some(UpdateLoadBalancerService {
                            http: None,
                            protocol: Some(hcloud::models::update_load_balancer_service::Protocol::Tcp),
                            listen_port: service.listen_port,
                            destination_port: Some(*destination_port),
                            proxyprotocol: Some(self.proxy_mode),
                            health_check: Some(Box::new(
                                hcloud::models::UpdateLoadBalancerServiceHealthCheck {
                                    protocol: Some(hcloud::models::update_load_balancer_service_health_check::Protocol::Tcp),
                                    http: None,
                                    interval: Some(self.check_interval),
                                    port: Some(*destination_port),
                                    retries: Some(self.retries),
                                    timeout: Some(self.timeout),
                                },
                            )),
                        }),
                    },
                )
                .await?;
            } else {
                tracing::info!(
                    "Deleting service that listens for port {} from load-balancer {}",
                    service.listen_port,
                    hcloud_balancer.name,
                );
                hcloud::apis::load_balancers_api::delete_service(
                    &self.hcloud_config,
                    DeleteServiceParams {
                        id: hcloud_balancer.id,
                        delete_service_request: Some(DeleteServiceRequest {
                            listen_port: service.listen_port,
                        }),
                    },
                )
                .await?;
            }
        }

        for (listen_port, destination_port) in &self.services {
            if !hcloud_balancer
                .services
                .iter()
                .any(|s| s.listen_port == *listen_port)
            {
                tracing::info!(
                    "Found missing service. Adding service that listens for port {}",
                    listen_port
                );
                hcloud::apis::load_balancers_api::add_service(
                    &self.hcloud_config,
                AddServiceParams {
                    id: hcloud_balancer.id,
                    body: Some(LoadBalancerService {
                        http: None,
                        listen_port: *listen_port,
                        destination_port: *destination_port,
                        protocol: hcloud::models::load_balancer_service::Protocol::Tcp,
                        proxyprotocol: self.proxy_mode,
                        health_check: Box::new(LoadBalancerServiceHealthCheck {
                            http: None,
                            interval: self.check_interval,
                            port: *destination_port,
                            protocol:
                                hcloud::models::load_balancer_service_health_check::Protocol::Tcp,
                            retries: self.retries,
                            timeout: self.timeout,
                        }),
                    }),
                },
            )
            .await?;
            }
        }
        Ok(())
    }

    /// Reconcile the targets of the load balancer.
    /// This method will compare the desired configuration of the targets
    /// with the current configuration of the targets in the load balancer.
    /// If the configuration does not match, the method will update the target.
    async fn reconcile_targets(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<()> {
        let max_targets =
            usize::try_from(hcloud_balancer.load_balancer_type.max_targets).unwrap_or(usize::MAX);
        let planned = plan_targets(&self.targets, max_targets);
        if planned.len() < self.targets.len()
            // While a type change is in flight the balancer still reports the old type,
            // whose limit says nothing about the type the service asked for.
            && hcloud_balancer.load_balancer_type.name == self.balancer_type
        {
            tracing::warn!(
                "Selected {} node(s), but a {} balancer holds at most {}. \
                 Use a bigger balancer type or externalTrafficPolicy: Local.",
                self.targets.len(),
                hcloud_balancer.load_balancer_type.name,
                max_targets,
            );
        }

        for target in &hcloud_balancer.targets {
            let Some(target_ip) = target.ip.clone() else {
                continue;
            };
            if !planned.contains(&target_ip.ip.as_str()) {
                tracing::info!("Removing target {}", target_ip.ip);
                hcloud::apis::load_balancers_api::remove_target(
                    &self.hcloud_config,
                    RemoveTargetParams {
                        id: hcloud_balancer.id,
                        remove_target_request: Some(RemoveTargetRequest {
                            ip: Some(target_ip),
                            ..Default::default()
                        }),
                    },
                )
                .await?;
            }
        }

        let mut live = 0_usize;
        let mut last_error = None;
        for ip in &planned {
            if hcloud_balancer
                .targets
                .iter()
                .any(|t| t.ip.as_ref().map(|i| i.ip.as_str()) == Some(*ip))
            {
                live += 1;
                continue;
            }
            tracing::info!("Adding target {}", ip);
            let added = hcloud::apis::load_balancers_api::add_target(
                &self.hcloud_config,
                AddTargetParams {
                    id: hcloud_balancer.id,
                    body: Some(LoadBalancerAddTarget {
                        ip: Some(Box::new(hcloud::models::LoadBalancerTargetIp {
                            ip: (*ip).to_string(),
                        })),
                        ..Default::default()
                    }),
                },
            )
            .await;
            // Hetzner rejects IPs outside the vSwitch subnet of the attached network,
            // which must not keep the remaining nodes out of the load balancer.
            match added {
                Ok(_) => live += 1,
                Err(error) => {
                    tracing::warn!("Cannot add target {ip}: {error}");
                    last_error = Some(error.to_string());
                }
            }
        }
        // A balancer left without a single target forwards nothing, so the service
        // must not be reported as ready. Counting what is live rather than what this
        // run attempted keeps a permanently rejected node from failing every run.
        if !planned.is_empty() && live == 0 {
            return Err(RobotLBError::HCloudError(format!(
                "No target could be added to load balancer {}: {}",
                self.name,
                last_error.unwrap_or_else(|| "no reason reported".to_string()),
            )));
        }
        Ok(())
    }

    /// Reconcile the load balancer algorithm.
    /// This method will compare the desired algorithm configuration
    /// and update it if it does not match the current configuration.
    async fn reconcile_algorithm(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<()> {
        if *hcloud_balancer.algorithm == self.algorithm.clone().into() {
            return Ok(());
        }
        tracing::info!(
            "Changing load balancer algorithm from {:?} to {:?}",
            hcloud_balancer.algorithm,
            self.algorithm
        );
        hcloud::apis::load_balancers_api::change_algorithm(
            &self.hcloud_config,
            ChangeAlgorithmParams {
                id: hcloud_balancer.id,
                body: Some(self.algorithm.clone().into()),
            },
        )
        .await?;
        Ok(())
    }

    /// Reconcile the load balancer type.
    async fn reconcile_lb_type(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<()> {
        if hcloud_balancer.load_balancer_type.name == self.balancer_type {
            return Ok(());
        }
        tracing::info!(
            "Changing load balancer type from {} to {}",
            hcloud_balancer.load_balancer_type.name,
            self.balancer_type
        );
        hcloud::apis::load_balancers_api::change_type_of_load_balancer(
            &self.hcloud_config,
            ChangeTypeOfLoadBalancerParams {
                id: hcloud_balancer.id,
                change_type_of_load_balancer_request: Some(ChangeTypeOfLoadBalancerRequest {
                    load_balancer_type: self.balancer_type.clone(),
                }),
            },
        )
        .await?;
        Ok(())
    }

    /// Reconcile the network of the load balancer.
    /// This method will compare the desired network configuration
    /// with the current network configuration of the load balancer.
    /// If the configuration does not match, the method will update the
    /// network configuration.
    async fn reconcile_network(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<()> {
        // If the network name is not provided, and laod balancer is not attached to any network,
        // we can skip this step.
        if self.network_name.is_none() && hcloud_balancer.private_net.is_empty() {
            return Ok(());
        }

        let desired_network = self.get_network().await?.map(|network| network.id);
        // If the network name is not provided, but the load balancer is attached to a network,
        // we need to detach it from the network.
        let mut contain_desired_network = false;
        if !hcloud_balancer.private_net.is_empty() {
            for private_net in &hcloud_balancer.private_net {
                let Some(private_net_id) = private_net.network else {
                    continue;
                };
                // The load balancer is attached to a target network.
                if desired_network == Some(private_net_id) {
                    // Specific IP was provided, we need to check if the IP is the same.
                    if self.private_ip.is_some() {
                        // if IPs match, we can leave everything as it is.
                        if private_net.ip == self.private_ip {
                            contain_desired_network = true;
                            continue;
                        }
                    } else {
                        // No specific IP was provided, we can leave everything as it is.
                        contain_desired_network = true;
                        continue;
                    }
                }
                tracing::info!("Detaching balancer from network {}", private_net_id);
                hcloud::apis::load_balancers_api::detach_load_balancer_from_network(
                    &self.hcloud_config,
                    DetachLoadBalancerFromNetworkParams {
                        id: hcloud_balancer.id,
                        detach_load_balancer_from_network_request: Some(
                            DetachLoadBalancerFromNetworkRequest {
                                network: private_net_id,
                            },
                        ),
                    },
                )
                .await?;
            }
        }
        if !contain_desired_network {
            let Some(network_id) = desired_network else {
                return Ok(());
            };
            tracing::info!("Attaching balancer to network {}", network_id);
            hcloud::apis::load_balancers_api::attach_load_balancer_to_network(
                &self.hcloud_config,
                AttachLoadBalancerToNetworkParams {
                    id: hcloud_balancer.id,
                    attach_load_balancer_to_network_request: Some(
                        AttachLoadBalancerToNetworkRequest {
                            ip: self.private_ip.clone(),
                            network: network_id,
                        },
                    ),
                },
            )
            .await?;
        }
        Ok(())
    }

    /// Cleanup the load balancer.
    /// This method will remove all the services and targets from the
    /// load balancer.
    pub async fn cleanup(&self) -> RobotLBResult<()> {
        let Some(hcloud_balancer) = self.get_hcloud_lb().await? else {
            return Ok(());
        };
        for service in &hcloud_balancer.services {
            tracing::info!(
                "Deleting service that listens for port {} from load-balancer {}",
                service.listen_port,
                hcloud_balancer.name,
            );
            hcloud::apis::load_balancers_api::delete_service(
                &self.hcloud_config,
                DeleteServiceParams {
                    id: hcloud_balancer.id,
                    delete_service_request: Some(DeleteServiceRequest {
                        listen_port: service.listen_port,
                    }),
                },
            )
            .await?;
        }
        for target in &hcloud_balancer.targets {
            if let Some(target_ip) = target.ip.clone() {
                tracing::info!("Removing target {}", target_ip.ip);
                hcloud::apis::load_balancers_api::remove_target(
                    &self.hcloud_config,
                    RemoveTargetParams {
                        id: hcloud_balancer.id,
                        remove_target_request: Some(RemoveTargetRequest {
                            ip: Some(target_ip),
                            ..Default::default()
                        }),
                    },
                )
                .await?;
            }
        }
        hcloud::apis::load_balancers_api::delete_load_balancer(
            &self.hcloud_config,
            DeleteLoadBalancerParams {
                id: hcloud_balancer.id,
            },
        )
        .await?;
        Ok(())
    }

    /// Get the load balancer from Hetzner Cloud.
    /// This method will try to find the load balancer with the name
    /// specified in the `LoadBalancer` struct.
    ///
    /// The method might return an error if the load balancer is not found
    /// or if there are multiple load balancers with the same name.
    async fn get_hcloud_lb(&self) -> RobotLBResult<Option<hcloud::models::LoadBalancer>> {
        let hcloud_balancers = hcloud::apis::load_balancers_api::list_load_balancers(
            &self.hcloud_config,
            ListLoadBalancersParams {
                name: Some(self.name.to_string()),
                ..Default::default()
            },
        )
        .await?;
        if hcloud_balancers.load_balancers.len() > 1 {
            tracing::warn!(
                "Found more than one balancer with name {}, skipping",
                self.name
            );
            return Err(RobotLBError::SkipService);
        }
        // Here we just return the first load balancer,
        // if it exists, otherwise we return None
        Ok(hcloud_balancers.load_balancers.into_iter().next())
    }

    /// Get or create the load balancer in Hetzner Cloud.
    ///
    /// this method will try to find the load balancer with the name
    /// specified in the `LoadBalancer` struct. If the load balancer
    /// is not found, the method will create a new load balancer
    /// with the specified configuration in service's annotations.
    async fn get_or_create_hcloud_lb(&self) -> RobotLBResult<hcloud::models::LoadBalancer> {
        let hcloud_lb = self.get_hcloud_lb().await?;
        if let Some(balancer) = hcloud_lb {
            return Ok(balancer);
        }

        let response = hcloud::apis::load_balancers_api::create_load_balancer(
            &self.hcloud_config,
            hcloud::apis::load_balancers_api::CreateLoadBalancerParams {
                create_load_balancer_request: Some(hcloud::models::CreateLoadBalancerRequest {
                    algorithm: Some(Box::new(self.algorithm.clone())),
                    labels: None,
                    load_balancer_type: self.balancer_type.clone(),
                    location: Some(self.location.clone()),
                    name: self.name.clone(),
                    network: None,
                    network_zone: None,
                    public_interface: Some(true),
                    services: Some(vec![]),
                    targets: Some(vec![]),
                }),
            },
        )
        .await;
        if let Err(e) = response {
            tracing::error!("Failed to create load balancer: {:?}", e);
            return Err(RobotLBError::HCloudError(format!(
                "Failed to create load balancer: {:?}",
                e
            )));
        }

        Ok(*response.unwrap().load_balancer)
    }

    /// Get the network from Hetzner Cloud.
    /// This method will try to find the network with the name
    /// specified in the `LoadBalancer` struct. It returns `None` only
    /// in case the network name is not provided. If the network was not found,
    /// the error is returned.
    async fn get_network(&self) -> RobotLBResult<Option<hcloud::models::Network>> {
        let Some(network_name) = self.network_name.clone() else {
            return Ok(None);
        };
        let response = hcloud::apis::networks_api::list_networks(
            &self.hcloud_config,
            ListNetworksParams {
                name: Some(network_name.clone()),
                ..Default::default()
            },
        )
        .await?;

        if response.networks.len() > 1 {
            tracing::warn!(
                "Found more than one network with name {}, skipping",
                network_name
            );
            return Err(RobotLBError::HCloudError(format!(
                "Found more than one network with name {}",
                network_name,
            )));
        }
        if response.networks.is_empty() {
            tracing::warn!("Network with name {} not found", network_name);
            return Err(RobotLBError::HCloudError(format!(
                "Network with name {} not found",
                network_name,
            )));
        }

        Ok(response.networks.into_iter().next())
    }
}

/// The targets a balancer should end up with: deduplicated, and trimmed to what the
/// balancer type holds. Sorted, so that a cluster larger than the limit keeps the same
/// targets from one reconciliation to the next instead of trading them back and forth.
fn plan_targets(desired: &[String], max_targets: usize) -> Vec<&str> {
    let mut planned = desired.iter().map(String::as_str).collect::<Vec<_>>();
    planned.sort_unstable();
    planned.dedup();
    planned.truncate(max_targets);
    planned
}

impl FromStr for LBAlgorithm {
    type Err = RobotLBError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "round-robin" => Ok(Self::RoundRobin),
            "least-connections" => Ok(Self::LeastConnections),
            _ => Err(RobotLBError::UnknownLBAlgorithm),
        }
    }
}

impl From<LBAlgorithm> for LoadBalancerAlgorithm {
    fn from(value: LBAlgorithm) -> Self {
        let r#type = match value {
            LBAlgorithm::RoundRobin => hcloud::models::load_balancer_algorithm::Type::RoundRobin,
            LBAlgorithm::LeastConnections => {
                hcloud::models::load_balancer_algorithm::Type::LeastConnections
            }
        };
        Self { r#type }
    }
}

#[cfg(test)]
mod tests {
    use super::plan_targets;

    #[test]
    fn targets_are_sorted_and_deduplicated() {
        let desired = vec![
            "192.168.100.4".to_string(),
            "192.168.100.2".to_string(),
            "192.168.100.4".to_string(),
        ];
        assert_eq!(
            plan_targets(&desired, 25),
            vec!["192.168.100.2", "192.168.100.4"]
        );
    }

    #[test]
    fn targets_beyond_the_balancer_limit_are_dropped() {
        let desired = (1..=30)
            .map(|host| format!("192.168.100.{host:03}"))
            .collect::<Vec<_>>();
        let planned = plan_targets(&desired, 25);
        assert_eq!(planned.len(), 25);
        assert_eq!(planned[0], "192.168.100.001");
        assert_eq!(planned[24], "192.168.100.025");
    }

    #[test]
    fn a_plan_within_the_limit_keeps_every_target() {
        let desired = vec!["192.168.100.2".to_string(), "192.168.100.3".to_string()];
        assert_eq!(plan_targets(&desired, 25).len(), 2);
    }
}
