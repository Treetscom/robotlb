use k8s_openapi::api::core::v1::Service;
use kube::{Resource, ResourceExt};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{BuildHasher, DefaultHasher, Hash, Hasher},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

/// What each service looked like after its last successful reconcile, so that a
/// watch event that changed nothing robotlb acts on costs no Hetzner requests.
#[derive(Default)]
pub struct ReconcileMemo {
    entries: Mutex<HashMap<String, (u64, Instant)>>,
}

impl ReconcileMemo {
    /// How long the recorded state stays valid, when the service is still in it.
    pub fn unchanged(&self, uid: &str, fingerprint: u64, now: Instant) -> Option<Duration> {
        let (recorded, until) = *self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(uid)?;
        if recorded != fingerprint {
            return None;
        }
        Some(until.saturating_duration_since(now)).filter(|left| !left.is_zero())
    }

    /// Keep the state for `valid_for`, the time until the reconcile is due anyway.
    pub fn record(&self, uid: &str, fingerprint: u64, now: Instant, valid_for: Duration) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(uid.to_string(), (fingerprint, now + valid_for));
    }

    /// Drop the entry of a service after a failed reconcile, or once robotlb no
    /// longer serves it.
    pub fn settle(&self, svc: &Service, succeeded: bool) {
        let serves =
            svc.meta().deletion_timestamp.is_none() && crate::is_robotlb_load_balancer(svc);
        if !(succeeded && serves) {
            if let Some(uid) = svc.uid() {
                self.forget(&uid);
            }
        }
    }

    pub fn forget(&self, uid: &str) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(uid);
    }
}

/// A hash of everything a reconcile acts on: the spec, the robotlb annotations, and
/// the targets and ports computed from them.
pub fn fingerprint<S: BuildHasher>(
    svc: &Service,
    targets: &[String],
    services: &HashMap<i32, i32, S>,
) -> u64 {
    // `DefaultHasher::new` uses fixed keys, unlike `RandomState`, so the same input
    // hashes the same on every call within the process.
    let mut hasher = DefaultHasher::new();
    k8s_openapi::serde_json::to_string(&svc.spec)
        .unwrap_or_default()
        .hash(&mut hasher);
    for annotation in svc
        .annotations()
        .iter()
        .filter(|(key, _)| key.starts_with("robotlb/"))
    {
        annotation.hash(&mut hasher);
    }
    targets.iter().collect::<BTreeSet<_>>().hash(&mut hasher);
    services
        .iter()
        .collect::<BTreeMap<_, _>>()
        .hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{fingerprint, ReconcileMemo};
    use crate::consts;
    use k8s_openapi::{
        api::core::v1::{
            LoadBalancerIngress, LoadBalancerStatus, Service, ServicePort, ServiceSpec,
            ServiceStatus,
        },
        apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time},
    };
    use kube::ResourceExt;
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    const RESYNC: Duration = Duration::from_secs(300);

    fn service() -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some("web".to_string()),
                uid: Some("uid-1".to_string()),
                annotations: Some(
                    [(
                        consts::LB_LOCATION_LABEL_NAME.to_string(),
                        "fsn1".to_string(),
                    )]
                    .into(),
                ),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                type_: Some("LoadBalancer".to_string()),
                ports: Some(vec![ServicePort {
                    port: 80,
                    node_port: Some(30080),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn fingerprint_of(svc: &Service, targets: &[&str], ports: &[(i32, i32)]) -> u64 {
        let targets = targets.iter().map(ToString::to_string).collect::<Vec<_>>();
        fingerprint(
            svc,
            &targets,
            &ports.iter().copied().collect::<HashMap<_, _>>(),
        )
    }

    fn print(svc: &Service) -> u64 {
        fingerprint_of(svc, &["10.0.0.1"], &[(80, 30080)])
    }

    #[test]
    fn an_unchanged_service_is_skipped_until_the_resync() {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        memo.record("uid-1", 7, start, RESYNC);
        assert_eq!(
            memo.unchanged("uid-1", 7, start + Duration::from_secs(100)),
            Some(Duration::from_secs(200))
        );
        assert_eq!(memo.unchanged("uid-1", 7, start + RESYNC), None);
        assert_eq!(
            memo.unchanged("uid-1", 7, start + RESYNC + Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn a_shorter_validity_ends_the_skip_sooner() {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        let retry = Duration::from_secs(30);
        memo.record("uid-1", 7, start, retry);
        assert_eq!(
            memo.unchanged("uid-1", 7, start + Duration::from_secs(10)),
            Some(Duration::from_secs(20))
        );
        assert_eq!(memo.unchanged("uid-1", 7, start + retry), None);
    }

    #[test]
    fn a_changed_or_unknown_service_is_reconciled() {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        assert_eq!(memo.unchanged("uid-1", 7, start), None);
        memo.record("uid-1", 7, start, RESYNC);
        assert_eq!(memo.unchanged("uid-1", 8, start), None);
        assert_eq!(memo.unchanged("uid-2", 7, start), None);
    }

    #[test]
    fn a_forgotten_service_is_reconciled() {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        memo.record("uid-1", 7, start, RESYNC);
        memo.forget("uid-1");
        assert_eq!(memo.unchanged("uid-1", 7, start), None);
    }

    #[test]
    fn a_new_record_restarts_the_resync() {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        memo.record("uid-1", 7, start, RESYNC);
        memo.record("uid-1", 8, start + RESYNC, RESYNC);
        assert_eq!(memo.unchanged("uid-1", 8, start + RESYNC), Some(RESYNC));
    }

    fn settled(svc: &Service, succeeded: bool) -> bool {
        let memo = ReconcileMemo::default();
        let start = Instant::now();
        memo.record("uid-1", 7, start, RESYNC);
        memo.settle(svc, succeeded);
        memo.unchanged("uid-1", 7, start).is_some()
    }

    #[test]
    fn a_successful_reconcile_keeps_the_entry() {
        assert!(settled(&service(), true));
    }

    #[test]
    fn a_failed_reconcile_drops_the_entry() {
        assert!(!settled(&service(), false));
    }

    #[test]
    fn a_released_or_deleted_service_drops_the_entry() {
        let mut svc = service();
        svc.spec.as_mut().unwrap().type_ = Some("ClusterIP".to_string());
        assert!(!settled(&svc, true));
        let mut svc = service();
        svc.spec.as_mut().unwrap().load_balancer_class = Some("other".to_string());
        assert!(!settled(&svc, true));
        let mut svc = service();
        svc.metadata.deletion_timestamp = Some(Time(k8s_openapi::chrono::Utc::now()));
        assert!(!settled(&svc, true));
    }

    #[test]
    fn the_fingerprint_is_stable() {
        assert_eq!(print(&service()), print(&service()));
    }

    #[test]
    fn a_spec_change_changes_the_fingerprint() {
        let mut svc = service();
        svc.spec.as_mut().unwrap().external_traffic_policy = Some("Local".to_string());
        assert_ne!(print(&svc), print(&service()));
    }

    #[test]
    fn a_robotlb_annotation_changes_the_fingerprint() {
        let mut svc = service();
        svc.annotations_mut().insert(
            consts::LB_LOCATION_LABEL_NAME.to_string(),
            "nbg1".to_string(),
        );
        assert_ne!(print(&svc), print(&service()));
        let mut svc = service();
        svc.annotations_mut().insert(
            consts::LB_PRIVATE_IP_LABEL_NAME.to_string(),
            "10.0.0.9".to_string(),
        );
        assert_ne!(print(&svc), print(&service()));
    }

    #[test]
    fn the_targets_change_the_fingerprint() {
        let svc = service();
        let one = fingerprint_of(&svc, &["10.0.0.1"], &[(80, 30080)]);
        let two = fingerprint_of(&svc, &["10.0.0.1", "10.0.0.2"], &[(80, 30080)]);
        let reordered = fingerprint_of(&svc, &["10.0.0.2", "10.0.0.1"], &[(80, 30080)]);
        assert_ne!(one, two);
        assert_eq!(two, reordered);
    }

    #[test]
    fn the_ports_change_the_fingerprint() {
        let svc = service();
        let one = fingerprint_of(&svc, &["10.0.0.1"], &[(80, 30080)]);
        let other = fingerprint_of(&svc, &["10.0.0.1"], &[(80, 30081)]);
        assert_ne!(one, other);
    }

    #[test]
    fn status_and_foreign_metadata_leave_the_fingerprint_alone() {
        let mut svc = service();
        svc.status = Some(ServiceStatus {
            load_balancer: Some(LoadBalancerStatus {
                ingress: Some(vec![LoadBalancerIngress {
                    ip: Some("192.0.2.1".to_string()),
                    ..Default::default()
                }]),
            }),
            ..Default::default()
        });
        svc.metadata.resource_version = Some("42".to_string());
        svc.annotations_mut().insert(
            "metallb.io/ip-allocated-from-pool".to_string(),
            "x".to_string(),
        );
        assert_eq!(print(&svc), print(&service()));
    }
}
