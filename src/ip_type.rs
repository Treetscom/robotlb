use std::str::FromStr;

use crate::error::RobotLBError;

/// IP Type set by the loadbancer in the status field.
#[derive(Clone, Debug, Copy, serde::Serialize)]
pub enum IPMode {
    /// Type of an IP when the traffic is routed
    /// with the loadbalancer IP and port.
    VIP,
    /// Type of an IP when the traffic is routed
    /// with the node IP.
    Proxy,
}

impl FromStr for IPMode {
    type Err = RobotLBError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "VIP" => Ok(Self::VIP),
            "Proxy" => Ok(Self::Proxy),
            _ => Err(RobotLBError::UnknownIPType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IPMode;
    use crate::error::RobotLBError;
    use std::str::FromStr;

    #[test]
    fn vip_and_proxy_are_parsed() {
        assert!(matches!(IPMode::from_str("VIP"), Ok(IPMode::VIP)));
        assert!(matches!(IPMode::from_str("Proxy"), Ok(IPMode::Proxy)));
    }

    // The values are case-sensitive, since the annotation is passed to the
    // status verbatim and anything else would end up in the service.
    #[test]
    fn an_unknown_ip_mode_is_rejected() {
        for input in ["vip", "proxy", "Vip", "EXTERNAL", ""] {
            assert!(
                matches!(IPMode::from_str(input), Err(RobotLBError::UnknownIPType)),
                "{input:?} must be rejected"
            );
        }
    }

    // The mode is written into the service status `ipMode` field as-is, so the
    // serialized form must match the accepted annotation values exactly.
    #[test]
    fn the_serialized_form_matches_the_accepted_values() {
        assert_eq!(
            k8s_openapi::serde_json::to_string(&IPMode::VIP).unwrap(),
            "\"VIP\""
        );
        assert_eq!(
            k8s_openapi::serde_json::to_string(&IPMode::Proxy).unwrap(),
            "\"Proxy\""
        );
    }
}
