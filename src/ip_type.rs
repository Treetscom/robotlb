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
