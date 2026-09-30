use hcloud::{
    apis::{
        configuration::Configuration as HcloudConfig,
        load_balancers_api::{
            AddServiceParams, AddTargetParams, AttachLoadBalancerToNetworkParams,
            ChangeAlgorithmParams, ChangeTypeOfLoadBalancerParams, DeleteLoadBalancerParams,
            DeleteServiceParams, DetachLoadBalancerFromNetworkParams, ListLoadBalancersParams,
            RemoveTargetParams, ReplaceLoadBalancerParams, UpdateServiceParams,
        },
        networks_api::ListNetworksParams,
    },
    models::{
        load_balancer_target, AttachLoadBalancerToNetworkRequest, ChangeTypeOfLoadBalancerRequest,
        DeleteServiceRequest, DetachLoadBalancerFromNetworkRequest, LoadBalancerAddTarget,
        LoadBalancerAlgorithm, LoadBalancerService, LoadBalancerServiceHealthCheck,
        RemoveTargetRequest, ReplaceLoadBalancerRequest, UpdateLoadBalancerService,
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

/// Retries after the first attempt, sleeping 1 unit, 2 units, ... between them.
const BUSY_RETRIES: u32 = 2;
const BUSY_RETRY_UNIT: std::time::Duration = std::time::Duration::from_secs(1);

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
#[derive(Debug, Default)]
pub struct LoadBalancer {
    pub name: String,
    /// The name earlier releases gave the balancer, when it differs from `name`.
    legacy_name: Option<String>,
    service_uid: String,
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

        let annotated_name = svc.annotations().get(consts::LB_NAME_LABEL_NAME);
        let legacy_name = annotated_name.is_none().then(|| svc.name_any());
        let name = annotated_name.cloned().unwrap_or_else(|| {
            default_name(
                context.config.cluster_name.as_deref(),
                &svc.name_any(),
                &svc.namespace().unwrap_or_default(),
            )
        });
        // The API server sets the UID on every object it stores.
        let service_uid = svc.uid().ok_or(RobotLBError::SkipService)?;

        let private_ip = svc
            .annotations()
            .get(consts::LB_PRIVATE_IP_LABEL_NAME)
            .cloned();

        Ok(Self {
            name,
            legacy_name,
            service_uid,
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

    /// A load balancer that is only good for `cleanup`, which finds the balancer by the
    /// service UID alone. The annotations are not read: one that does not parse must
    /// not keep the service from being released.
    pub fn for_release(svc: &Service, hcloud_config: HcloudConfig) -> RobotLBResult<Self> {
        Ok(Self {
            service_uid: svc.uid().ok_or(RobotLBError::SkipService)?,
            hcloud_config,
            ..Default::default()
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
    /// Returns the balancer, and whether some of its targets could not be added.
    #[tracing::instrument(skip(self), fields(lb_name = tracing::field::Empty))]
    pub async fn reconcile(&self) -> RobotLBResult<(hcloud::models::LoadBalancer, bool)> {
        let hcloud_balancer = self.get_or_create_hcloud_lb().await?;
        // An adopted balancer keeps its name, which may differ from `self.name`.
        tracing::Span::current().record("lb_name", hcloud_balancer.name.as_str());
        self.reconcile_algorithm(&hcloud_balancer).await?;
        self.reconcile_lb_type(&hcloud_balancer).await?;
        self.reconcile_network(&hcloud_balancer).await?;
        self.reconcile_services(&hcloud_balancer).await?;
        let targets_missing = self.reconcile_targets(&hcloud_balancer).await?;
        Ok((hcloud_balancer, targets_missing))
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
                if self.service_is_current(service, *destination_port) {
                    continue;
                }
                tracing::info!(
                    "Desired service configuration for port {} does not match current configuration. Updating ...",
                    service.listen_port,
                );
                retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
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
                })
                .await?;
            } else {
                tracing::info!(
                    "Deleting service that listens for port {} from load-balancer {}",
                    service.listen_port,
                    hcloud_balancer.name,
                );
                retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
                    hcloud::apis::load_balancers_api::delete_service(
                        &self.hcloud_config,
                        DeleteServiceParams {
                            id: hcloud_balancer.id,
                            delete_service_request: Some(DeleteServiceRequest {
                                listen_port: service.listen_port,
                            }),
                        },
                    )
                })
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
                retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
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
                })
                .await?;
            }
        }
        Ok(())
    }

    fn service_is_current(&self, service: &LoadBalancerService, destination_port: i32) -> bool {
        service.destination_port == destination_port
            && service.health_check.port == destination_port
            && service.health_check.interval == self.check_interval
            && service.health_check.retries == self.retries
            && service.health_check.timeout == self.timeout
            && service.proxyprotocol == self.proxy_mode
            && service.http.is_none()
            && service.health_check.protocol
                == hcloud::models::load_balancer_service_health_check::Protocol::Tcp
    }

    /// Reconcile the targets of the load balancer.
    /// This method will compare the desired configuration of the targets
    /// with the current configuration of the targets in the load balancer.
    /// If the configuration does not match, the method will update the target.
    async fn reconcile_targets(
        &self,
        hcloud_balancer: &hcloud::models::LoadBalancer,
    ) -> RobotLBResult<bool> {
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
                retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
                    hcloud::apis::load_balancers_api::remove_target(
                        &self.hcloud_config,
                        RemoveTargetParams {
                            id: hcloud_balancer.id,
                            remove_target_request: Some(RemoveTargetRequest {
                                ip: Some(target_ip.clone()),
                                ..Default::default()
                            }),
                        },
                    )
                })
                .await?;
            }
        }

        let mut live = 0_usize;
        let mut last_error = None;
        let mut retries = BUSY_RETRIES;
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
            let added = retry_temporary(retries, BUSY_RETRY_UNIT, || {
                hcloud::apis::load_balancers_api::add_target(
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
            })
            .await;
            // A lock is on the whole balancer and a failing API stays failing for a while,
            // so once the retries ran out for one target the remaining ones would only
            // wait the same time for nothing.
            if added
                .as_ref()
                .is_err_and(crate::error::is_temporary_rejection)
            {
                retries = 0;
            }
            // Hetzner rejects IPs outside the vSwitch subnet of the attached network,
            // which must not keep the remaining nodes out of the load balancer.
            match added {
                Ok(_) => live += 1,
                Err(error) if crate::error::is_target_already_defined(&error) => live += 1,
                // Every further call would be rejected too and only drain the budget.
                Err(error) if crate::error::is_rate_limit_response(&error) => {
                    return Err(error.into());
                }
                Err(error) => {
                    tracing::warn!("Cannot add target {ip}: {error}");
                    last_error = Some(crate::error::describe(&error));
                }
            }
        }
        // A balancer left without a single target forwards nothing, so the service
        // must not be reported as ready. Counting what is live rather than what this
        // run attempted keeps a permanently rejected node from failing every run.
        if !planned.is_empty() && live == 0 {
            return Err(RobotLBError::HCloudError(format!(
                "No target could be added to load balancer {}: {}",
                hcloud_balancer.name,
                last_error.unwrap_or_else(|| "no reason reported".to_string()),
            )));
        }
        Ok(live < planned.len())
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
        retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
            hcloud::apis::load_balancers_api::change_algorithm(
                &self.hcloud_config,
                ChangeAlgorithmParams {
                    id: hcloud_balancer.id,
                    body: Some(self.algorithm.clone().into()),
                },
            )
        })
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
        retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
            hcloud::apis::load_balancers_api::change_type_of_load_balancer(
                &self.hcloud_config,
                ChangeTypeOfLoadBalancerParams {
                    id: hcloud_balancer.id,
                    change_type_of_load_balancer_request: Some(ChangeTypeOfLoadBalancerRequest {
                        load_balancer_type: self.balancer_type.clone(),
                    }),
                },
            )
        })
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
                retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
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
                })
                .await?;
            }
        }
        if !contain_desired_network {
            let Some(network_id) = desired_network else {
                return Ok(());
            };
            tracing::info!("Attaching balancer to network {}", network_id);
            retry_temporary(BUSY_RETRIES, BUSY_RETRY_UNIT, || {
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
            })
            .await?;
        }
        Ok(())
    }

    /// Delete the balancer labelled for the service. A balancer is never deleted by
    /// its name: another service may have created one under it in the meantime.
    pub async fn cleanup(&self) -> RobotLBResult<()> {
        let Some(hcloud_balancer) = self.find_hcloud_lb(Purpose::Release).await? else {
            return Ok(());
        };
        hcloud::apis::load_balancers_api::delete_load_balancer(
            &self.hcloud_config,
            DeleteLoadBalancerParams {
                id: hcloud_balancer.id,
            },
        )
        .await?;
        Ok(())
    }

    /// Find the balancer of the service: the one labelled with its UID, or, when
    /// reconciling, an unlabelled balancer from an earlier release that it adopts.
    async fn find_hcloud_lb(
        &self,
        purpose: Purpose,
    ) -> RobotLBResult<Option<hcloud::models::LoadBalancer>> {
        let selector = owner_selector(&self.service_uid);
        let labelled = self
            .list_hcloud_lbs(ListLoadBalancersParams {
                label_selector: Some(selector.clone()),
                ..Default::default()
            })
            .await?;
        if let Some(balancer) = single(labelled, &selector)? {
            return Ok(Some(balancer));
        }
        for (name, legacy) in candidate_names(purpose, &self.name, self.legacy_name.as_deref())? {
            let named = self
                .list_hcloud_lbs(ListLoadBalancersParams {
                    name: Some(name.to_string()),
                    ..Default::default()
                })
                .await?;
            let Some(balancer) = single(named, name)? else {
                continue;
            };
            match decide(&balancer, &self.service_uid, &self.targets, legacy) {
                Decision::Use => return Ok(Some(balancer)),
                Decision::Adopt => return self.adopt(balancer).await.map(Some),
                Decision::Skip => {
                    tracing::info!("Load balancer {name} is not this service's, skipping");
                }
                Decision::Foreign(owner) => {
                    return Err(RobotLBError::ForeignBalancer {
                        name: name.to_string(),
                        owner,
                    })
                }
                Decision::Unrecognised => {
                    return Err(RobotLBError::UnrecognisedBalancer {
                        name: name.to_string(),
                        uid: self.service_uid.clone(),
                    })
                }
                Decision::NoNodes => {
                    return Err(RobotLBError::NoNodesToRecogniseBalancer(name.to_string()))
                }
            }
        }
        Ok(None)
    }

    async fn list_hcloud_lbs(
        &self,
        params: ListLoadBalancersParams,
    ) -> RobotLBResult<Vec<hcloud::models::LoadBalancer>> {
        Ok(
            hcloud::apis::load_balancers_api::list_load_balancers(&self.hcloud_config, params)
                .await?
                .load_balancers,
        )
    }

    async fn adopt(
        &self,
        balancer: hcloud::models::LoadBalancer,
    ) -> RobotLBResult<hcloud::models::LoadBalancer> {
        tracing::info!("Adopting load balancer {}", balancer.name);
        let response = hcloud::apis::load_balancers_api::replace_load_balancer(
            &self.hcloud_config,
            ReplaceLoadBalancerParams {
                id: balancer.id,
                replace_load_balancer_request: Some(ReplaceLoadBalancerRequest {
                    labels: Some(owner_labels(&balancer.labels, &self.service_uid)),
                    name: None,
                }),
            },
        )
        .await?;
        Ok(*response.load_balancer)
    }

    /// Get or create the load balancer in Hetzner Cloud. A new balancer carries the
    /// service UID label from the start.
    async fn get_or_create_hcloud_lb(&self) -> RobotLBResult<hcloud::models::LoadBalancer> {
        if let Some(balancer) = self.find_hcloud_lb(Purpose::Reconcile).await? {
            return Ok(balancer);
        }

        let response = hcloud::apis::load_balancers_api::create_load_balancer(
            &self.hcloud_config,
            hcloud::apis::load_balancers_api::CreateLoadBalancerParams {
                create_load_balancer_request: Some(hcloud::models::CreateLoadBalancerRequest {
                    algorithm: Some(Box::new(self.algorithm.clone())),
                    labels: Some(owner_labels(&HashMap::new(), &self.service_uid)),
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
        .await?;

        Ok(*response.load_balancer)
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

/// Service names, namespaces and the cluster name are DNS labels: at most 63
/// characters and no dots, so the name cannot be split two ways. Without a cluster
/// name it fits the 128 Hetzner allows. A longer one is cut and ends in a 64-bit
/// hash of the whole name, so long names that share the kept part still differ.
fn default_name(cluster: Option<&str>, service: &str, namespace: &str) -> String {
    let name = cluster.map_or_else(
        || format!("{service}.{namespace}"),
        |cluster| format!("{cluster}.{service}.{namespace}"),
    );
    if name.len() <= MAX_NAME_LEN {
        return name;
    }
    // DNS labels are ASCII, so the cut falls on a character boundary.
    let hash = format!("-{:016x}", fnv1a64(&name));
    format!("{}{hash}", &name[..MAX_NAME_LEN - hash.len()])
}

const MAX_NAME_LEN: usize = 128;

/// FNV-1a, 64 bit: the hash ends up in names balancers are looked up by, so it has
/// to stay the same across releases, which std's hashers do not promise.
fn fnv1a64(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Hetzner checks a name against `^\S(.*\S)?$` and 1 to 128 characters and answers
/// a create with a name that fails with 422. The pattern comes from its API
/// schema, where regexes are ECMA-262: `.` does not match a line terminator.
fn validate_name(name: &str) -> RobotLBResult<()> {
    // ECMA `\s` differs from Rust whitespace in U+FEFF (in) and U+0085 (out).
    let not_space = |c: char| c != '\u{feff}' && (!c.is_whitespace() || c == '\u{85}');
    let edges_ok = name.starts_with(not_space) && name.ends_with(not_space);
    let one_line = !name.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
    if edges_ok && one_line && name.chars().count() <= MAX_NAME_LEN {
        Ok(())
    } else {
        Err(RobotLBError::InvalidBalancerName(name.to_string()))
    }
}

/// Without dots, the cluster name cannot end in the middle of a `<cluster>.<service>`
/// prefix of another cluster.
pub fn parse_cluster_name(value: &str) -> Result<String, String> {
    let alphanumeric = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let valid = (1..=63).contains(&value.len())
        && value.chars().all(|c| alphanumeric(c) || c == '-')
        && value.starts_with(alphanumeric)
        && value.ends_with(alphanumeric);
    if valid {
        Ok(value.to_string())
    } else {
        Err("must be a DNS label: 1 to 63 lowercase letters, digits or '-', starting and ending with a letter or digit".to_string())
    }
}

fn owner_selector(uid: &str) -> String {
    format!("{}={uid}", consts::LB_OWNER_LABEL)
}

/// Hetzner replaces the whole label set of a balancer, so the existing labels go along.
fn owner_labels(existing: &HashMap<String, String>, uid: &str) -> HashMap<String, String> {
    let mut labels = existing.clone();
    labels.insert(consts::LB_OWNER_LABEL.to_string(), uid.to_string());
    labels
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Reconcile,
    Release,
}

/// Names to look a balancer up by when none carries the service UID, each marked
/// whether it is the legacy name. A release never goes by name.
fn candidate_names<'a>(
    purpose: Purpose,
    name: &'a str,
    legacy_name: Option<&'a str>,
) -> RobotLBResult<Vec<(&'a str, bool)>> {
    if purpose == Purpose::Release {
        return Ok(vec![]);
    }
    validate_name(name)?;
    Ok(legacy_name
        .map(|legacy| (legacy, true))
        .into_iter()
        .chain([(name, false)])
        .collect())
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Use,
    Adopt,
    /// Look for the balancer under the next name.
    Skip,
    /// Labelled for the service with this UID.
    Foreign(String),
    Unrecognised,
    /// Unlabelled, and the service has no nodes to compare its targets with.
    NoNodes,
}

/// Balancers of earlier releases carry no label. One is taken over only when it looks
/// like robotlb made it for this service: IP targets only, at least one of them a node
/// of the service. A balancer of another team or cluster points elsewhere.
fn decide(
    balancer: &hcloud::models::LoadBalancer,
    uid: &str,
    desired_targets: &[String],
    legacy: bool,
) -> Decision {
    match balancer.labels.get(consts::LB_OWNER_LABEL) {
        Some(owner) if owner == uid => return Decision::Use,
        Some(_) if legacy => return Decision::Skip,
        Some(owner) => return Decision::Foreign(owner.clone()),
        None => {}
    }
    let ip_only = balancer
        .targets
        .iter()
        .all(|target| target.r#type == load_balancer_target::Type::Ip);
    if ip_only && desired_targets.is_empty() {
        // Skipping an old balancer here would replace it with a new one and a new
        // address while the service merely waits for its pods.
        return Decision::NoNodes;
    }
    let on_service_nodes = balancer.targets.iter().any(|target| {
        target
            .ip
            .as_ref()
            .is_some_and(|ip| desired_targets.contains(&ip.ip))
    });
    if ip_only && on_service_nodes {
        Decision::Adopt
    } else if legacy {
        Decision::Skip
    } else {
        Decision::Unrecognised
    }
}

fn single<T>(mut found: Vec<T>, what: &str) -> RobotLBResult<Option<T>> {
    if found.len() > 1 {
        return Err(RobotLBError::AmbiguousBalancer(what.to_string()));
    }
    Ok(found.pop())
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

/// Call `call`, and call it again after `unit`, `2 * unit`, ... while Hetzner rejects it
/// for a temporary reason, up to `retries` more times. Returns the last outcome.
async fn retry_temporary<T, E, F, Fut>(
    retries: u32,
    unit: std::time::Duration,
    mut call: F,
) -> Result<T, hcloud::apis::Error<E>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, hcloud::apis::Error<E>>>,
{
    let mut attempt = 0_u32;
    loop {
        match call().await {
            Err(error) if attempt < retries && crate::error::is_temporary_rejection(&error) => {
                attempt += 1;
                tracing::debug!(
                    "Rejected temporarily, retry {attempt}: {}",
                    crate::error::describe(&error)
                );
                tokio::time::sleep(unit * attempt).await;
            }
            outcome => return outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_names, decide, default_name, fnv1a64, owner_labels, owner_selector,
        parse_cluster_name, plan_targets, single, validate_name, Decision, LoadBalancer, Purpose,
    };
    use crate::{consts, error::RobotLBError};
    use hcloud::apis::configuration::Configuration as HcloudConfig;
    use hcloud::models::{
        load_balancer_target, LoadBalancer as HcloudBalancer, LoadBalancerTarget,
        LoadBalancerTargetIp,
    };
    use k8s_openapi::{api::core::v1::Service, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use std::collections::HashMap;

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

    use super::retry_temporary;
    use hcloud::apis::{load_balancers_api::AddTargetError, Error, ResponseContent};
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        time::Duration,
    };

    fn rejected(status: u16) -> Error<AddTargetError> {
        Error::ResponseError(ResponseContent {
            status: status.try_into().unwrap(),
            content: if status == 423 {
                r#"{"error": {"code": "locked", "message": "item is locked"}}"#.to_string()
            } else {
                String::new()
            },
            entity: None,
        })
    }

    /// Answers with `statuses` in order, then with success; returns the outcome and the call count.
    async fn run(retries: u32, statuses: &[u16]) -> (Result<(), Error<AddTargetError>>, u32) {
        let calls = AtomicU32::new(0);
        let result = retry_temporary(retries, Duration::ZERO, || {
            let call = calls.fetch_add(1, Ordering::Relaxed);
            let answer = statuses
                .get(call as usize)
                .map_or(Ok(()), |s| Err(rejected(*s)));
            async move { answer }
        })
        .await;
        (result, calls.load(Ordering::Relaxed))
    }

    #[tokio::test]
    async fn a_locked_balancer_is_retried_until_it_accepts() {
        let (result, calls) = run(2, &[423, 423]).await;
        assert!(result.is_ok());
        assert_eq!(calls, 3);
    }

    #[tokio::test]
    async fn a_balancer_that_stays_locked_gets_the_first_call_and_the_retries_only() {
        let (result, calls) = run(2, &[423; 10]).await;
        assert!(result.is_err());
        assert_eq!(calls, 3);
    }

    #[tokio::test]
    async fn no_retries_means_a_single_call() {
        let (result, calls) = run(0, &[423; 10]).await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn a_permanent_rejection_is_not_retried() {
        let (result, calls) = run(2, &[422; 10]).await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn a_rate_limit_is_not_retried() {
        let (result, calls) = run(2, &[429; 10]).await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn a_success_is_not_repeated() {
        let (result, calls) = run(2, &[]).await;
        assert!(result.is_ok());
        assert_eq!(calls, 1);
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

    #[test]
    fn the_default_name_carries_the_namespace() {
        assert_eq!(default_name(None, "web", "shop"), "web.shop");
        assert_ne!(
            default_name(None, "web", "shop"),
            default_name(None, "web", "blog")
        );
    }

    // Both parts are DNS labels of at most 63 characters, Hetzner takes 128.
    #[test]
    fn the_longest_default_name_fits_hetzner() {
        let part = "a".repeat(63);
        assert_eq!(default_name(None, &part, &part).len(), 127);
    }

    #[test]
    fn the_default_name_starts_with_the_cluster_name() {
        assert_eq!(default_name(Some("prod"), "web", "shop"), "prod.web.shop");
    }

    #[test]
    fn a_long_default_name_is_cut_to_the_limit_and_hashed() {
        let part = "a".repeat(63);
        let full = format!("{part}.{part}.{part}");
        let name = default_name(Some(&part), &part, &part);
        assert_eq!(name.len(), 128);
        assert_eq!(name, format!("{}-{:016x}", &full[..111], fnv1a64(&full)));
        assert!(validate_name(&name).is_ok());
    }

    #[test]
    fn a_default_name_of_128_characters_is_kept_and_one_of_129_is_cut() {
        let cluster = "a".repeat(63);
        let service = "b".repeat(62);
        let kept = default_name(Some(&cluster), &service, "c");
        assert_eq!(kept, format!("{cluster}.{service}.c"));
        assert_eq!(kept.len(), 128);
        let full = format!("{cluster}.{service}.cc");
        let cut = default_name(Some(&cluster), &service, "cc");
        assert_eq!(cut, format!("{}-{:016x}", &full[..111], fnv1a64(&full)));
    }

    #[test]
    fn long_default_names_differing_after_the_cut_stay_different() {
        let part = "a".repeat(63);
        assert_ne!(
            default_name(Some(&part), &part, &"b".repeat(63)),
            default_name(Some(&part), &part, &"c".repeat(63)),
        );
    }

    // The hash is part of a name balancers are looked up by, it must never change.
    #[test]
    fn the_name_hash_is_fnv1a_64() {
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_owner_selector_matches_the_service_uid() {
        assert_eq!(owner_selector("uid-1"), "robotlb/service-uid=uid-1");
    }

    // Hetzner replaces the whole label set, so labels set by others must be sent back.
    #[test]
    fn owner_labels_keep_existing_labels() {
        let existing = HashMap::from([
            ("team".to_string(), "web".to_string()),
            (consts::LB_OWNER_LABEL.to_string(), "uid-2".to_string()),
        ]);
        let labels = owner_labels(&existing, "uid-1");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels["team"], "web");
        assert_eq!(labels[consts::LB_OWNER_LABEL], "uid-1");
    }

    fn ip_target(ip: &str) -> LoadBalancerTarget {
        LoadBalancerTarget {
            r#type: load_balancer_target::Type::Ip,
            ip: Some(Box::new(LoadBalancerTargetIp { ip: ip.to_string() })),
            ..Default::default()
        }
    }

    fn balancer(owner: Option<&str>, targets: Vec<LoadBalancerTarget>) -> HcloudBalancer {
        HcloudBalancer {
            labels: owner
                .map(|uid| HashMap::from([(consts::LB_OWNER_LABEL.to_string(), uid.to_string())]))
                .unwrap_or_default(),
            targets,
            ..Default::default()
        }
    }

    fn nodes() -> Vec<String> {
        vec!["192.0.2.1".to_string(), "192.0.2.2".to_string()]
    }

    #[test]
    fn a_balancer_labelled_for_the_service_is_used() {
        let lb = balancer(Some("uid-1"), vec![]);
        assert_eq!(decide(&lb, "uid-1", &nodes(), true), Decision::Use);
        assert_eq!(decide(&lb, "uid-1", &nodes(), false), Decision::Use);
    }

    #[test]
    fn a_balancer_labelled_for_another_service_is_never_taken() {
        let lb = balancer(Some("uid-2"), vec![ip_target("192.0.2.1")]);
        assert_eq!(decide(&lb, "uid-1", &nodes(), true), Decision::Skip);
        assert_eq!(
            decide(&lb, "uid-1", &nodes(), false),
            Decision::Foreign("uid-2".to_string())
        );
    }

    #[test]
    fn an_unlabelled_balancer_on_the_service_nodes_is_adopted() {
        let lb = balancer(None, vec![ip_target("192.0.2.9"), ip_target("192.0.2.2")]);
        assert_eq!(decide(&lb, "uid-1", &nodes(), true), Decision::Adopt);
        assert_eq!(decide(&lb, "uid-1", &nodes(), false), Decision::Adopt);
    }

    #[test]
    fn an_unlabelled_balancer_elsewhere_is_not_adopted() {
        let lb = balancer(None, vec![ip_target("198.51.100.1")]);
        assert_eq!(decide(&lb, "uid-1", &nodes(), true), Decision::Skip);
        assert_eq!(
            decide(&lb, "uid-1", &nodes(), false),
            Decision::Unrecognised
        );
        let empty = balancer(None, vec![]);
        assert_eq!(decide(&empty, "uid-1", &nodes(), true), Decision::Skip);
    }

    // robotlb only ever adds IP targets.
    #[test]
    fn an_unlabelled_balancer_with_server_targets_is_not_adopted() {
        let server = LoadBalancerTarget {
            r#type: load_balancer_target::Type::Server,
            ..Default::default()
        };
        let lb = balancer(None, vec![ip_target("192.0.2.1"), server]);
        assert_eq!(
            decide(&lb, "uid-1", &nodes(), false),
            Decision::Unrecognised
        );
        // No node could make it robotlb's, so a service without nodes need not wait.
        assert_eq!(decide(&lb, "uid-1", &[], true), Decision::Skip);
    }

    #[test]
    fn an_unlabelled_balancer_is_not_skipped_while_the_service_has_no_nodes() {
        let lb = balancer(None, vec![ip_target("192.0.2.1")]);
        assert_eq!(decide(&lb, "uid-1", &[], true), Decision::NoNodes);
        assert_eq!(decide(&lb, "uid-1", &[], false), Decision::NoNodes);
    }

    #[test]
    fn a_release_never_looks_a_balancer_up_by_name() {
        assert!(candidate_names(Purpose::Release, "web.shop", Some("web"))
            .unwrap()
            .is_empty());
        assert_eq!(
            candidate_names(Purpose::Reconcile, "web.shop", Some("web")).unwrap(),
            vec![("web", true), ("web.shop", false)]
        );
        assert_eq!(
            candidate_names(Purpose::Reconcile, "custom", None).unwrap(),
            vec![("custom", false)]
        );
    }

    #[test]
    fn an_invalid_name_is_never_looked_up_or_created() {
        assert!(matches!(
            candidate_names(Purpose::Reconcile, " web", None),
            Err(RobotLBError::InvalidBalancerName(_))
        ));
        // A release carries no name and must not fail on it.
        assert!(candidate_names(Purpose::Release, "", None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn more_than_one_match_is_an_error() {
        assert!(single(Vec::<i32>::new(), "x").unwrap().is_none());
        assert_eq!(single(vec![1], "x").unwrap(), Some(1));
        assert!(matches!(
            single(vec![1, 2], "x"),
            Err(RobotLBError::AmbiguousBalancer(_))
        ));
    }
    #[test]
    fn a_release_ignores_annotations_that_do_not_parse() {
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
        let lb = LoadBalancer::for_release(&svc, HcloudConfig::default()).unwrap();
        assert_eq!(lb.service_uid, "uid-1");
    }

    #[test]
    fn a_name_hetzner_rejects_is_invalid() {
        for name in [
            "",
            " web",
            "web ",
            "\tweb",
            "web\n",
            "a\nb",
            "\u{feff}web",
            &"a".repeat(129),
        ] {
            assert!(
                matches!(
                    validate_name(name),
                    Err(RobotLBError::InvalidBalancerName(_))
                ),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_name_hetzner_takes_is_valid() {
        for name in [
            "w",
            "custom name",
            "web\u{85}",
            &"a".repeat(128),
            &"ä".repeat(128),
        ] {
            assert!(validate_name(name).is_ok(), "{name:?}");
        }
    }

    // The name only matters when no balancer carries the service UID, so a labelled
    // balancer keeps being managed whatever the annotation says.
    #[tokio::test]
    async fn an_invalid_balancer_name_does_not_stop_a_service_from_loading() {
        use clap::Parser;
        let config =
            crate::config::OperatorConfig::try_parse_from(["robotlb", "--hcloud-token", "t"])
                .unwrap();
        let client =
            kube::Client::try_from(kube::Config::new("http://127.0.0.1:1".parse().unwrap()))
                .unwrap();
        let context = crate::CurrentContext::new(client, config, HcloudConfig::default());
        let svc = Service {
            metadata: ObjectMeta {
                uid: Some("uid-1".to_string()),
                annotations: Some(
                    [(consts::LB_NAME_LABEL_NAME.to_string(), " web".to_string())].into(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        let lb = LoadBalancer::try_from_svc(&svc, &context).unwrap();
        assert_eq!(lb.name, " web");
    }

    fn context_with_cluster_name() -> crate::CurrentContext {
        use clap::Parser;
        let config = crate::config::OperatorConfig::try_parse_from([
            "robotlb",
            "--hcloud-token",
            "t",
            "--cluster-name",
            "prod",
        ])
        .unwrap();
        let client =
            kube::Client::try_from(kube::Config::new("http://127.0.0.1:1".parse().unwrap()))
                .unwrap();
        crate::CurrentContext::new(client, config, HcloudConfig::default())
    }

    #[tokio::test]
    async fn the_cluster_name_prefixes_the_default_name() {
        let svc = Service {
            metadata: ObjectMeta {
                uid: Some("uid-1".to_string()),
                name: Some("web".to_string()),
                namespace: Some("shop".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let lb = LoadBalancer::try_from_svc(&svc, &context_with_cluster_name()).unwrap();
        assert_eq!(lb.name, "prod.web.shop");
    }

    #[tokio::test]
    async fn the_cluster_name_leaves_an_annotated_name_alone() {
        let annotated = "a".repeat(128);
        let svc = Service {
            metadata: ObjectMeta {
                uid: Some("uid-1".to_string()),
                name: Some("web".to_string()),
                namespace: Some("shop".to_string()),
                annotations: Some(
                    [(consts::LB_NAME_LABEL_NAME.to_string(), annotated.clone())].into(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        let lb = LoadBalancer::try_from_svc(&svc, &context_with_cluster_name()).unwrap();
        assert_eq!(lb.name, annotated);
    }

    fn parse_cluster_name_arg(
        cluster_name: &str,
    ) -> Result<crate::config::OperatorConfig, clap::Error> {
        use clap::Parser;
        crate::config::OperatorConfig::try_parse_from([
            "robotlb".to_string(),
            "--hcloud-token=t".to_string(),
            format!("--cluster-name={cluster_name}"),
        ])
    }

    #[test]
    fn the_cluster_name_is_unset_by_default() {
        use clap::Parser;
        let config =
            crate::config::OperatorConfig::try_parse_from(["robotlb", "--hcloud-token", "t"])
                .unwrap();
        assert_eq!(config.cluster_name, None);
    }

    #[test]
    fn a_dns_label_is_a_valid_cluster_name() {
        for name in ["a", "prod", "eu-1", "0", &"a".repeat(63)] {
            assert_eq!(parse_cluster_name(name).as_deref(), Ok(name));
            assert_eq!(
                parse_cluster_name_arg(name)
                    .unwrap()
                    .cluster_name
                    .as_deref(),
                Some(name)
            );
        }
    }

    #[test]
    fn a_cluster_name_that_is_not_a_dns_label_is_rejected() {
        for name in [
            "",
            "Prod",
            "eu.prod",
            "-prod",
            "prod-",
            "pr_od",
            "prød",
            &"a".repeat(64),
        ] {
            assert!(parse_cluster_name(name).is_err(), "{name:?}");
            let error = parse_cluster_name_arg(name).unwrap_err();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ValueValidation,
                "{name:?}"
            );
        }
    }
}
