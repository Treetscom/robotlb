use clap::Parser;
use tracing::level_filters::LevelFilter;

#[derive(Debug, Clone, Parser)]
pub struct OperatorConfig {
    /// `HCloud` API token.
    #[arg(short = 't', long, env = "ROBOTLB_HCLOUD_TOKEN")]
    pub hcloud_token: String,

    /// Name of the cluster, put in front of default balancer names so that clusters
    /// sharing a Hetzner project do not pick the same ones. A DNS label: lowercase
    /// letters, digits and `-`, at most 63 characters.
    #[arg(long, env = "ROBOTLB_CLUSTER_NAME", value_parser = crate::lb::parse_cluster_name)]
    pub cluster_name: Option<String>,

    /// Default network to use for load balancers.
    /// If not set, then only network from the service annotation will be used.
    #[arg(long, env = "ROBOTLB_DEFAULT_NETWORK", default_value = None)]
    pub default_network: Option<String>,

    /// If enabled, the operator will try to find target nodes based on the service's traffic policy, and under `Local` on the nodes serving the service's endpoints.
    /// If disabled, the operator will try to find target nodes based on the node selector.
    #[arg(long, env = "ROBOTLB_DYNAMIC_NODE_SELECTOR", default_value = "true")]
    pub dynamic_node_selector: bool,

    /// Default load balancer healthcheck retries cound.
    #[arg(long, env = "ROBOTLB_DEFAULT_LB_RETRIES", default_value = "3")]
    pub default_lb_retries: i32,

    /// Default load balancer healthcheck timeout.
    #[arg(long, env = "ROBOTLB_DEFAULT_LB_TIMEOUT", default_value = "10")]
    pub default_lb_timeout: i32,

    /// Default load balancer healhcheck interval.
    #[arg(long, env = "ROBOTLB_DEFAULT_LB_INTERVAL", default_value = "15")]
    pub default_lb_interval: i32,

    /// Default location of a load balancer.
    /// https://docs.hetzner.com/cloud/general/locations/
    #[arg(long, env = "ROBOTLB_DEFAULT_LB_LOCATION", default_value = "hel1")]
    pub default_lb_location: String,

    /// Type of a load balancer. It differs in price, number of connections,
    /// target servers, etc. The default value is the smallest balancer.
    /// https://docs.hetzner.com/cloud/load-balancers/overview#pricing
    #[arg(long, env = "ROBOTLB_DEFAULT_LB_TYPE", default_value = "lb11")]
    pub default_balancer_type: String,

    /// Default load balancer algorithm.
    /// Possible values:
    /// * `least-connections`
    /// * `round-robin`
    /// https://docs.hetzner.com/cloud/load-balancers/overview#load-balancers
    #[arg(
        long,
        env = "ROBOTLB_DEFAULT_LB_ALGORITHM",
        default_value = "least-connections"
    )]
    pub default_lb_algorithm: String,

    /// Default load balancer proxy mode. If enabled, the load balancer will
    /// act as a proxy for the target servers. The default value is `false`.
    /// https://docs.hetzner.com/cloud/load-balancers/faq/#what-does-proxy-protocol-mean-and-should-i-enable-it
    #[arg(
        long,
        env = "ROBOTLB_DEFAULT_LB_PROXY_MODE_ENABLED",
        default_value = "false"
    )]
    pub default_lb_proxy_mode_enabled: bool,

    /// Whether to enable IPv6 ingress for the load balancer.
    /// If enabled, the load balancer's IPv6 will be attached to the service as an external IP along with IPv4.
    #[arg(long, env = "ROBOTLB_IPV6_INGRESS", default_value = "false")]
    pub ipv6_ingress: bool,

    /// Seconds between reconciliations of a service that nothing changed. Node changes,
    /// and endpoint changes of Local services, trigger a reconciliation on their own;
    /// this interval bounds how long a change made to a balancer outside robotlb survives.
    /// A service whose balancer refused a target is retried within 30 seconds.
    #[arg(
        long,
        env = "ROBOTLB_RESYNC_INTERVAL",
        default_value = "300",
        value_parser = clap::value_parser!(u64).range(1..=31_536_000)
    )]
    pub resync_interval: u64,

    // Log level of the operator.
    #[arg(long, env = "ROBOTLB_LOG_LEVEL", default_value = "INFO")]
    pub log_level: LevelFilter,
}

#[cfg(test)]
mod tests {
    use super::OperatorConfig;
    use clap::Parser;

    fn parse(resync: &str) -> Result<OperatorConfig, clap::Error> {
        OperatorConfig::try_parse_from([
            "robotlb",
            "--hcloud-token",
            "t",
            "--resync-interval",
            resync,
        ])
    }

    // Zero would requeue every successful reconcile right away, spending Hetzner
    // API requests on every service all the time. The controller's delay queue
    // panics on delays past about two years.
    #[test]
    fn the_resync_interval_must_be_between_a_second_and_a_year() {
        assert!(parse("0").is_err());
        assert!(parse("31536001").is_err());
        assert_eq!(parse("31536000").unwrap().resync_interval, 31_536_000);
        assert_eq!(parse("1").unwrap().resync_interval, 1);
    }
}
