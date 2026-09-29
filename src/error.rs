use thiserror::Error;

pub type RobotLBResult<T> = Result<T, RobotLBError>;

#[derive(Debug, Error)]
pub enum RobotLBError {
    #[error("Cannot parse node filter: {0}")]
    InvalidNodeFilter(String),
    #[error("Unsupported service type")]
    UnsupportedServiceType,
    #[error("Service was skipped")]
    SkipService,
    #[error("Cannot parse integer value: {0}")]
    PaseIntError(#[from] std::num::ParseIntError),
    #[error("Cannot parse boolean value: {0}")]
    PaseBoolError(#[from] std::str::ParseBoolError),
    #[error("HCloud error: {0}")]
    HCloudError(String),
    #[error("Kube error: {0}")]
    KubeError(#[from] kube::Error),
    #[error("Unknown LoadBalancing alorithm")]
    UnknownLBAlgorithm,
    #[error("Cannot get target nodes, because the service has no selector")]
    ServiceWithoutSelector,
    #[error(
        "No TCP port of the service has a nodePort, so the load balancer has nothing to forward"
    )]
    NoExposablePorts,
    #[error(
        "Load balancer '{name}' is labelled for the service with UID {owner}, so robotlb leaves it alone. Set another name through the robotlb/balancer annotation"
    )]
    ForeignBalancer { name: String, owner: String },
    #[error(
        "Load balancer '{name}' has no {label} label and does not look like a balancer robotlb made for this service (IP targets only, at least one of them a node of the service), so robotlb leaves it alone. If it belongs to this service: hcloud load-balancer add-label '{name}' {label}={uid}",
        label = crate::consts::LB_OWNER_LABEL
    )]
    UnrecognisedBalancer { name: String, uid: String },
    #[error(
        "Load balancer '{0}' has no {label} label, and whether it belongs to this service cannot be told before the service has target nodes",
        label = crate::consts::LB_OWNER_LABEL
    )]
    NoNodesToRecogniseBalancer(String),
    #[error("More than one load balancer matches {0}")]
    AmbiguousBalancer(String),
    #[error("Hetzner Cloud API rate limit reached, the pause ends in {}s", .0.as_millis().div_ceil(1000))]
    RateLimited(std::time::Duration),

    // HCloud API errors
    #[error("Cannot attach load balancer to a network. Reason: {}", describe(.0))]
    HCloudLBAttachToNetworkError(
        #[from]
        hcloud::apis::Error<hcloud::apis::load_balancers_api::AttachLoadBalancerToNetworkError>,
    ),
    #[error("Cannot detach load balancer from network. Reason: {}", describe(.0))]
    HcloudLBDetachFromNetworkError(
        #[from]
        hcloud::apis::Error<hcloud::apis::load_balancers_api::DetachLoadBalancerFromNetworkError>,
    ),
    #[error("Cannot add load balancer target. Reason: {}", describe(.0))]
    HcloudLBAddTargetError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::AddTargetError>,
    ),
    #[error("Cannot remove load balancer target. Reason: {}", describe(.0))]
    HcloudLBRemoveTargetError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::RemoveTargetError>,
    ),
    #[error("Cannot add service to load balancer. Reason: {}", describe(.0))]
    HcloudLBAddServiceError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::AddServiceError>,
    ),
    #[error("Cannot remove service from load balancer. Reason: {}", describe(.0))]
    HcloudLBRemoveServiceError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::DeleteServiceError>,
    ),
    #[error("Cannot create load balancer. Reason: {}", describe(.0))]
    HcloudLBCreateError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::CreateLoadBalancerError>,
    ),
    #[error("Cannot delete load balancer. Reason: {}", describe(.0))]
    HcloudLBDeleteError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::DeleteLoadBalancerError>,
    ),
    #[error("Cannot get load balancer. Reason: {}", describe(.0))]
    HcloudLBGetError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::GetLoadBalancerError>,
    ),
    #[error("Cannot update service. Reason: {}", describe(.0))]
    HcloudLBUpdateServiceError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::UpdateServiceError>,
    ),
    #[error("Cannot change type of load balancer. Reason: {}", describe(.0))]
    HcloudLBChangeType(
        #[from]
        hcloud::apis::Error<hcloud::apis::load_balancers_api::ChangeTypeOfLoadBalancerError>,
    ),
    #[error("Cannot change algorithm of load balancer. Reason: {}", describe(.0))]
    HcloudLBChangeAlgorithm(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::ChangeAlgorithmError>,
    ),
    #[error("Cannot label load balancer. Reason: {}", describe(.0))]
    HcloudLBReplaceError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::ReplaceLoadBalancerError>,
    ),
    #[error("Cannot list networks. Reason: {}", describe(.0))]
    HcloudListNetworksError(
        #[from] hcloud::apis::Error<hcloud::apis::networks_api::ListNetworksError>,
    ),
    #[error("Cannot list load balancers. Reason: {}", describe(.0))]
    HcloudListLoadBalancersError(
        #[from] hcloud::apis::Error<hcloud::apis::load_balancers_api::ListLoadBalancersError>,
    ),
}

impl RobotLBError {
    /// Whether Hetzner rejected the call because the project ran out of API requests.
    #[must_use]
    pub fn is_rate_limited(&self) -> bool {
        // No wildcard arm: a new variant must be sorted into one of the two groups.
        match self {
            Self::HCloudLBAttachToNetworkError(error) => is_rate_limit_response(error),
            Self::HcloudLBDetachFromNetworkError(error) => is_rate_limit_response(error),
            Self::HcloudLBAddTargetError(error) => is_rate_limit_response(error),
            Self::HcloudLBRemoveTargetError(error) => is_rate_limit_response(error),
            Self::HcloudLBAddServiceError(error) => is_rate_limit_response(error),
            Self::HcloudLBRemoveServiceError(error) => is_rate_limit_response(error),
            Self::HcloudLBCreateError(error) => is_rate_limit_response(error),
            Self::HcloudLBDeleteError(error) => is_rate_limit_response(error),
            Self::HcloudLBGetError(error) => is_rate_limit_response(error),
            Self::HcloudLBUpdateServiceError(error) => is_rate_limit_response(error),
            Self::HcloudLBChangeType(error) => is_rate_limit_response(error),
            Self::HcloudLBChangeAlgorithm(error) => is_rate_limit_response(error),
            Self::HcloudLBReplaceError(error) => is_rate_limit_response(error),
            Self::HcloudListNetworksError(error) => is_rate_limit_response(error),
            Self::HcloudListLoadBalancersError(error) => is_rate_limit_response(error),
            Self::InvalidNodeFilter(_)
            | Self::UnsupportedServiceType
            | Self::SkipService
            | Self::PaseIntError(_)
            | Self::PaseBoolError(_)
            | Self::HCloudError(_)
            | Self::KubeError(_)
            | Self::UnknownLBAlgorithm
            | Self::ServiceWithoutSelector
            | Self::NoExposablePorts
            | Self::ForeignBalancer { .. }
            | Self::UnrecognisedBalancer { .. }
            | Self::NoNodesToRecogniseBalancer(_)
            | Self::AmbiguousBalancer(_)
            | Self::RateLimited(_) => false,
        }
    }
}

/// Whether Hetzner answered 429 because the project ran out of API requests.
#[must_use]
pub fn is_rate_limit_response<T>(error: &hcloud::apis::Error<T>) -> bool {
    matches!(error, hcloud::apis::Error::ResponseError(response) if response.status.as_u16() == 429)
}

/// One line with the HTTP status and the error Hetzner reported, if the body carries one.
#[must_use]
pub fn describe<T>(error: &hcloud::apis::Error<T>) -> String {
    let hcloud::apis::Error::ResponseError(response) = error else {
        return error.to_string();
    };
    let body =
        k8s_openapi::serde_json::from_str::<k8s_openapi::serde_json::Value>(&response.content).ok();
    let reported = body.as_ref().and_then(|body| {
        let error = body.get("error")?;
        Some(format!(
            "{}: {}",
            error.get("code")?.as_str()?,
            error.get("message")?.as_str()?
        ))
    });
    reported.map_or_else(
        || response.status.to_string(),
        |reported| {
            let one_line = reported.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("{}: {one_line}", response.status)
        },
    )
}

/// Whether Hetzner refused the call for a reason that may pass on its own.
///
/// That is a balancer locked by a running action (error code `locked`, HTTP 423), a
/// resource changed during the request (`conflict`, HTTP 409), Robot being briefly
/// unavailable (`robot_unavailable`), or a failure on the API side (5xx).
/// A 429 is not temporary here: the rate limit gate owns it.
#[must_use]
pub fn is_temporary_rejection<T>(error: &hcloud::apis::Error<T>) -> bool {
    let hcloud::apis::Error::ResponseError(response) = error else {
        return false;
    };
    matches!(
        error_code(error).as_deref(),
        Some("locked" | "conflict" | "robot_unavailable")
    ) || response.status.is_server_error()
}

/// Whether Hetzner refused to add a target because the balancer already has it, which
/// is what a retry gets after a call that Hetzner applied but answered with a 5xx.
#[must_use]
pub fn is_target_already_defined<T>(error: &hcloud::apis::Error<T>) -> bool {
    error_code(error).as_deref() == Some("target_already_defined")
}

fn error_code<T>(error: &hcloud::apis::Error<T>) -> Option<String> {
    let hcloud::apis::Error::ResponseError(response) = error else {
        return None;
    };
    let body =
        k8s_openapi::serde_json::from_str::<k8s_openapi::serde_json::Value>(&response.content)
            .ok()?;
    body.pointer("/error/code")?.as_str().map(str::to_owned)
}

/// Replace the API token, or any prefix of it long enough to identify it, with a marker.
/// Hetzner quotes the start of the token in some error messages.
#[must_use]
pub fn redact(message: &str, token: &str) -> String {
    const SHORTEST_PREFIX: usize = 8;
    let mut redacted = message.to_string();
    for len in (SHORTEST_PREFIX..=token.len()).rev() {
        if let Some(prefix) = token.get(..len) {
            redacted = redacted.replace(prefix, "[REDACTED]");
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::{describe, is_rate_limit_response, redact, RobotLBError};
    use hcloud::apis::{load_balancers_api::ListLoadBalancersError, Error, ResponseContent};

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ01";

    fn response_error(status: u16, content: &str) -> Error<ListLoadBalancersError> {
        Error::ResponseError(ResponseContent {
            status: status.try_into().unwrap(),
            content: content.to_string(),
            entity: None,
        })
    }

    #[test]
    fn a_token_prefix_is_redacted() {
        let message = format!("limit reached for token {}", &TOKEN[..32]);
        assert_eq!(
            redact(&message, TOKEN),
            "limit reached for token [REDACTED]"
        );
    }

    #[test]
    fn the_whole_token_is_redacted() {
        assert_eq!(redact(&format!("x{TOKEN}y"), TOKEN), "x[REDACTED]y");
    }

    #[test]
    fn a_message_without_the_token_is_kept() {
        assert_eq!(redact("status 500", TOKEN), "status 500");
    }

    #[test]
    fn an_empty_token_redacts_nothing() {
        assert_eq!(redact("status 500", ""), "status 500");
    }

    #[test]
    fn the_hetzner_message_is_described_on_one_line() {
        let error = response_error(
            429,
            r#"{"error": {"code": "rate_limit_exceeded", "message": "limit\n  reached"}}"#,
        );
        assert_eq!(
            describe(&error),
            "429 Too Many Requests: rate_limit_exceeded: limit reached"
        );
    }

    #[test]
    fn a_body_that_is_not_json_falls_back_to_the_status() {
        let error = response_error(502, "<html>bad gateway</html>");
        assert_eq!(describe(&error), "502 Bad Gateway");
    }

    #[test]
    fn a_pause_shorter_than_a_second_is_not_reported_as_over() {
        let error = RobotLBError::RateLimited(std::time::Duration::from_millis(300));
        assert!(error.to_string().ends_with("in 1s"));
    }

    #[test]
    fn a_429_is_a_rate_limit() {
        let error = RobotLBError::from(response_error(429, ""));
        assert!(error.is_rate_limited());
    }

    #[test]
    fn only_a_429_response_is_a_rate_limit() {
        assert!(is_rate_limit_response(&response_error(429, "")));
        // Hetzner answers 422 for a target outside the vSwitch subnet.
        assert!(!is_rate_limit_response(&response_error(422, "")));
    }

    #[test]
    fn other_statuses_are_not_a_rate_limit() {
        assert!(!RobotLBError::from(response_error(500, "")).is_rate_limited());
        assert!(!RobotLBError::SkipService.is_rate_limited());
        assert!(!RobotLBError::AmbiguousBalancer("web".to_string()).is_rate_limited());
    }

    #[test]
    fn an_unrecognised_balancer_names_the_handover_command() {
        let error = RobotLBError::UnrecognisedBalancer {
            name: "custom name".to_string(),
            uid: "uid-1".to_string(),
        };
        assert!(!error.is_rate_limited());
        assert!(error
            .to_string()
            .contains("hcloud load-balancer add-label 'custom name' robotlb/service-uid=uid-1"));
    }

    // Relabelling would take the balancer from a service that may still use it.
    #[test]
    fn a_balancer_of_another_service_names_its_owner_and_no_handover() {
        let error = RobotLBError::ForeignBalancer {
            name: "web".to_string(),
            owner: "uid-2".to_string(),
        };
        assert!(!error.is_rate_limited());
        assert!(error.to_string().contains("uid-2"));
        assert!(!error.to_string().contains("add-label"));
    }

    #[test]
    fn a_service_without_nodes_is_told_to_wait() {
        let error = RobotLBError::NoNodesToRecogniseBalancer("web".to_string());
        assert!(!error.is_rate_limited());
        assert!(!error.to_string().contains("add-label"));
    }

    #[test]
    fn a_locked_balancer_is_temporary() {
        let body = r#"{"error": {"code": "locked", "message": "item is locked"}}"#;
        assert!(super::is_temporary_rejection(&response_error(423, body)));
        assert!(super::is_temporary_rejection(&response_error(409, body)));
    }

    #[test]
    fn a_conflicting_change_is_temporary() {
        let body = r#"{"error": {"code": "conflict", "message": "please retry"}}"#;
        assert!(super::is_temporary_rejection(&response_error(409, body)));
    }

    #[test]
    fn an_unavailable_robot_is_temporary() {
        let body = r#"{"error": {"code": "robot_unavailable", "message": "retry later"}}"#;
        assert!(super::is_temporary_rejection(&response_error(422, body)));
    }

    #[test]
    fn a_protected_balancer_is_permanent() {
        let body = r#"{"error": {"code": "protected", "message": "protected"}}"#;
        assert!(!super::is_temporary_rejection(&response_error(423, body)));
    }

    #[test]
    fn a_server_error_is_temporary() {
        assert!(super::is_temporary_rejection(&response_error(500, "")));
        assert!(super::is_temporary_rejection(&response_error(
            503,
            "<html></html>"
        )));
    }

    #[test]
    fn a_rate_limit_is_not_temporary() {
        let body = r#"{"error": {"code": "rate_limit_exceeded", "message": "slow down"}}"#;
        assert!(!super::is_temporary_rejection(&response_error(429, body)));
    }

    #[test]
    fn a_target_outside_the_subnet_is_permanent() {
        let body = r#"{"error": {"code": "invalid_input", "message": "not in subnet"}}"#;
        assert!(!super::is_temporary_rejection(&response_error(422, body)));
        assert!(!super::is_temporary_rejection(&response_error(404, "")));
    }

    #[test]
    fn an_already_defined_target_is_recognised() {
        let body = r#"{"error": {"code": "target_already_defined", "message": "already added"}}"#;
        assert!(super::is_target_already_defined(&response_error(409, body)));
        assert!(super::is_target_already_defined(&response_error(422, body)));
    }

    #[test]
    fn other_rejections_are_not_an_already_defined_target() {
        let body = r#"{"error": {"code": "locked", "message": "item is locked"}}"#;
        assert!(!super::is_target_already_defined(&response_error(
            423, body
        )));
        assert!(!super::is_target_already_defined(&response_error(500, "")));
        let error: Error<ListLoadBalancersError> =
            Error::Serde(k8s_openapi::serde_json::from_str::<()>("x").unwrap_err());
        assert!(!super::is_target_already_defined(&error));
    }

    #[test]
    fn a_transport_failure_is_not_temporary() {
        let error: Error<ListLoadBalancersError> =
            Error::Serde(k8s_openapi::serde_json::from_str::<()>("x").unwrap_err());
        assert!(!super::is_temporary_rejection(&error));
    }
}
