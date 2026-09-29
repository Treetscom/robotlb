use k8s_openapi::api::{
    core::v1::{Node, Service},
    discovery::v1::EndpointSlice,
};
use kube::{
    runtime::{reflector::ObjectRef, watcher::Event},
    ResourceExt,
};
use std::{
    collections::{hash_map::DefaultHasher, BTreeSet, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Mutex,
};

/// Tells which node events change what a balancer may target.
///
/// Kubelets rewrite their node's status on every heartbeat, and reconciling every
/// service on each of those would call Hetzner for every service every few minutes
/// per node.
#[derive(Debug, Default)]
pub struct NodeChanges {
    seen: HashMap<String, u64>,
    relisted: HashMap<String, u64>,
    synced: bool,
}

impl NodeChanges {
    /// Whether the event calls for reconciling every service.
    pub fn observe(&mut self, event: &Event<Node>) -> bool {
        match event {
            Event::Init => {
                self.relisted.clear();
                false
            }
            Event::InitApply(node) => {
                self.relisted.insert(node.name_any(), fingerprint(node));
                false
            }
            Event::InitDone => {
                // The first listing counts as a change too: a node may have changed
                // after the controller's first reconciles listed it.
                let changed = !self.synced || self.relisted != self.seen;
                self.seen = std::mem::take(&mut self.relisted);
                self.synced = true;
                changed
            }
            Event::Apply(node) => {
                let print = fingerprint(node);
                self.seen.insert(node.name_any(), print) != Some(print)
            }
            Event::Delete(node) => self.seen.remove(&node.name_any()).is_some(),
        }
    }
}

/// The node fields target selection reads: labels, cordoning, readiness and addresses.
fn fingerprint(node: &Node) -> u64 {
    let mut hasher = DefaultHasher::new();
    node.labels().hash(&mut hasher);
    node.spec
        .as_ref()
        .and_then(|spec| spec.unschedulable)
        .hash(&mut hasher);
    if let Some(status) = &node.status {
        status
            .conditions
            .iter()
            .flatten()
            .find(|condition| condition.type_ == "Ready")
            .map(|condition| condition.status.as_str())
            .hash(&mut hasher);
        for address in status.addresses.iter().flatten() {
            (address.type_.as_str(), address.address.as_str()).hash(&mut hasher);
        }
    }
    hasher.finish()
}

type SliceKey = (String, String);

/// Which endpoint nodes a service's targets follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follows {
    /// Targets come from the service's pods, which ignore readiness.
    AllNodes,
    /// Targets come from the ready endpoints.
    ReadyNodes,
}

/// Tells which endpoint slice changes move balancer targets.
///
/// A slice is rewritten on every readiness flip and address change, but only the
/// nodes of its endpoints matter to a balancer. Reacting to every rewrite would call
/// Hetzner for a busy service every few seconds.
///
/// The controller's watch mapper cannot tell a deletion from an update, so a
/// separate watch tracks which slices exist and reports deletions.
#[derive(Debug, Default)]
pub struct SliceChanges {
    state: Mutex<SliceState>,
}

#[derive(Debug, Default)]
struct SliceState {
    live: HashSet<SliceKey>,
    relisted: HashSet<SliceKey>,
    nodes: HashMap<SliceKey, (Follows, BTreeSet<String>)>,
}

impl SliceChanges {
    /// Whether the nodes of the slice changed. A slice the watch does not know to
    /// exist, just created or already deleted, always counts as changed.
    pub fn observe(&self, slice: &EndpointSlice, follows: Follows) -> bool {
        let key = slice_key(slice);
        let nodes = slice_nodes(slice, follows);
        let mut state = self.lock();
        if !state.live.contains(&key) {
            return true;
        }
        let recorded = (follows, nodes);
        state.nodes.insert(key, recorded.clone()) != Some(recorded)
    }

    /// Drop what is known about the slice's nodes, for a service no longer observed.
    pub fn forget(&self, slice: &EndpointSlice) {
        self.lock().nodes.remove(&slice_key(slice));
    }

    /// Record a watch event. Returns whether a slice with endpoints was deleted,
    /// which calls for reconciling every service.
    /// Generic over the object, so the watch can carry only slice metadata.
    pub fn track<K: kube::Resource>(&self, event: &Event<K>) -> bool {
        let mut state = self.lock();
        match event {
            Event::Init => {
                state.relisted.clear();
                false
            }
            Event::InitApply(slice) => {
                state.relisted.insert(slice_key(slice));
                false
            }
            Event::InitDone => {
                let live = std::mem::take(&mut state.relisted);
                let mut dropped_endpoints = false;
                state.nodes.retain(|key, (_, nodes)| {
                    let kept = live.contains(key);
                    dropped_endpoints |= !kept && !nodes.is_empty();
                    kept
                });
                state.live = live;
                dropped_endpoints
            }
            Event::Apply(slice) => {
                state.live.insert(slice_key(slice));
                false
            }
            Event::Delete(slice) => {
                let key = slice_key(slice);
                state.live.remove(&key);
                state
                    .nodes
                    .remove(&key)
                    .is_some_and(|(_, nodes)| !nodes.is_empty())
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SliceState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn slice_key<K: kube::Resource>(slice: &K) -> SliceKey {
    (slice.namespace().unwrap_or_default(), slice.name_any())
}

fn slice_nodes(slice: &EndpointSlice, follows: Follows) -> BTreeSet<String> {
    slice
        .endpoints
        .iter()
        // The API treats a missing ready condition as ready.
        .filter(|endpoint| {
            follows == Follows::AllNodes
                || endpoint.conditions.as_ref().and_then(|c| c.ready) != Some(false)
        })
        .filter_map(|endpoint| endpoint.node_name.clone())
        .collect()
}

#[must_use]
pub fn slice_service(slice: &EndpointSlice) -> Option<ObjectRef<Service>> {
    let name = slice.labels().get("kubernetes.io/service-name")?;
    let namespace = slice.namespace()?;
    Some(ObjectRef::new(name).within(&namespace))
}

#[cfg(test)]
mod tests {
    use super::{slice_service, Follows, NodeChanges, SliceChanges};
    use k8s_openapi::{
        api::{
            core::v1::{Node, NodeAddress, NodeCondition, NodeSpec, NodeStatus},
            discovery::v1::{Endpoint, EndpointConditions, EndpointSlice},
        },
        apimachinery::pkg::apis::meta::v1::ObjectMeta,
    };
    use kube::runtime::{reflector::ObjectRef, watcher::Event};
    use std::collections::BTreeMap;

    fn node(name: &str, ready: &str) -> Node {
        Node {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(NodeSpec::default()),
            status: Some(NodeStatus {
                addresses: Some(vec![NodeAddress {
                    type_: "InternalIP".to_string(),
                    address: "192.0.2.10".to_string(),
                }]),
                conditions: Some(vec![NodeCondition {
                    type_: "Ready".to_string(),
                    status: ready.to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn synced(nodes: &[Node]) -> NodeChanges {
        let mut changes = NodeChanges::default();
        assert!(!changes.observe(&Event::Init));
        for node in nodes {
            assert!(!changes.observe(&Event::InitApply(node.clone())));
        }
        changes.observe(&Event::InitDone);
        changes
    }

    // A node that changed between the controller's first reconciles and this watch's
    // first listing would otherwise only be picked up by the resync.
    #[test]
    fn the_first_listing_triggers_a_reconcile() {
        let mut changes = NodeChanges::default();
        assert!(!changes.observe(&Event::Init));
        assert!(!changes.observe(&Event::InitApply(node("a", "True"))));
        assert!(changes.observe(&Event::InitDone));
    }

    #[test]
    fn a_heartbeat_triggers_nothing() {
        let mut changes = synced(&[node("a", "True")]);
        let mut beat = node("a", "True");
        if let Some(conditions) = beat.status.as_mut().and_then(|s| s.conditions.as_mut()) {
            conditions[0].message = Some("kubelet is posting ready status".to_string());
        }
        assert!(!changes.observe(&Event::Apply(beat)));
    }

    #[test]
    fn readiness_labels_addresses_and_cordoning_trigger_a_reconcile() {
        let mut changes = synced(&[node("a", "True")]);
        assert!(changes.observe(&Event::Apply(node("a", "False"))));
        assert!(!changes.observe(&Event::Apply(node("a", "False"))));

        let mut labelled = node("a", "False");
        labelled.metadata.labels = Some(BTreeMap::from([(
            "node.kubernetes.io/exclude-from-external-load-balancers".to_string(),
            String::new(),
        )]));
        assert!(changes.observe(&Event::Apply(labelled.clone())));

        let mut moved = labelled;
        moved.status.as_mut().unwrap().addresses.as_mut().unwrap()[0].address =
            "192.0.2.11".to_string();
        assert!(changes.observe(&Event::Apply(moved.clone())));

        let mut cordoned = moved;
        cordoned.spec.as_mut().unwrap().unschedulable = Some(true);
        assert!(changes.observe(&Event::Apply(cordoned)));
    }

    #[test]
    fn added_and_removed_nodes_trigger_a_reconcile() {
        let mut changes = synced(&[node("a", "True")]);
        assert!(changes.observe(&Event::Apply(node("b", "True"))));
        assert!(changes.observe(&Event::Delete(node("b", "True"))));
        assert!(!changes.observe(&Event::Delete(node("unknown", "True"))));
    }

    #[test]
    fn a_node_gone_from_a_relisting_triggers_a_reconcile() {
        let mut changes = synced(&[node("a", "True"), node("b", "True")]);
        assert!(!changes.observe(&Event::Init));
        assert!(!changes.observe(&Event::InitApply(node("a", "True"))));
        assert!(changes.observe(&Event::InitDone));
    }

    #[test]
    fn a_relisting_triggers_only_when_nodes_changed_meanwhile() {
        let mut changes = synced(&[node("a", "True")]);
        assert!(!changes.observe(&Event::Init));
        assert!(!changes.observe(&Event::InitApply(node("a", "True"))));
        assert!(!changes.observe(&Event::InitDone));

        assert!(!changes.observe(&Event::Init));
        assert!(!changes.observe(&Event::InitApply(node("a", "False"))));
        assert!(changes.observe(&Event::InitDone));
    }

    #[test]
    fn an_endpoint_slice_points_at_its_service() {
        let slice = EndpointSlice {
            metadata: ObjectMeta {
                name: Some("web-abcde".to_string()),
                namespace: Some("shop".to_string()),
                labels: Some(BTreeMap::from([(
                    "kubernetes.io/service-name".to_string(),
                    "web".to_string(),
                )])),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            slice_service(&slice),
            Some(ObjectRef::new("web").within("shop"))
        );
    }

    #[test]
    fn a_slice_without_a_service_points_nowhere() {
        assert_eq!(slice_service(&EndpointSlice::default()), None);
    }

    fn slice(endpoints: &[(&str, &str, bool)]) -> EndpointSlice {
        EndpointSlice {
            metadata: ObjectMeta {
                name: Some("web-abcde".to_string()),
                namespace: Some("shop".to_string()),
                ..Default::default()
            },
            endpoints: endpoints
                .iter()
                .map(|(address, node, ready)| Endpoint {
                    addresses: vec![(*address).to_string()],
                    node_name: Some((*node).to_string()),
                    conditions: Some(EndpointConditions {
                        ready: Some(*ready),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn tracked(initial: &EndpointSlice, follows: Follows) -> SliceChanges {
        let changes = SliceChanges::default();
        assert!(!changes.track(&Event::<EndpointSlice>::Init));
        assert!(!changes.track(&Event::InitApply(initial.clone())));
        assert!(!changes.track(&Event::<EndpointSlice>::InitDone));
        changes.observe(initial, follows);
        changes
    }

    #[test]
    fn a_slice_created_after_the_listing_is_tracked_like_a_listed_one() {
        let changes = tracked(&slice(&[]), Follows::ReadyNodes);
        let created = slice(&[("10.0.0.1", "a", true)]);
        let mut created = created;
        created.metadata.name = Some("web-fghij".to_string());
        assert!(!changes.track(&Event::Apply(created.clone())));
        assert!(changes.observe(&created, Follows::ReadyNodes));
        assert!(!changes.observe(&created, Follows::ReadyNodes));
        assert!(changes.track(&Event::Delete(created)));
    }

    #[test]
    fn a_slice_not_known_to_exist_triggers_a_reconcile() {
        let changes = SliceChanges::default();
        assert!(changes.observe(&slice(&[("10.0.0.1", "a", true)]), Follows::ReadyNodes));
    }

    // Replicas flapping readiness or changing addresses on the same nodes rewrite the
    // slice all the time, and none of that moves a balancer target.
    #[test]
    fn endpoint_churn_on_the_same_nodes_triggers_nothing() {
        for follows in [Follows::AllNodes, Follows::ReadyNodes] {
            let changes = tracked(
                &slice(&[("10.0.0.1", "a", true), ("10.0.0.2", "a", true)]),
                follows,
            );
            let moved = slice(&[("10.0.0.3", "a", true), ("10.0.0.2", "a", true)]);
            assert!(!changes.observe(&moved, follows));
            let flapped = slice(&[("10.0.0.3", "a", false), ("10.0.0.2", "a", true)]);
            assert!(!changes.observe(&flapped, follows));
        }
    }

    #[test]
    fn endpoint_targets_follow_the_last_ready_endpoint_of_a_node() {
        let changes = tracked(&slice(&[("10.0.0.1", "a", true)]), Follows::ReadyNodes);
        let added = slice(&[("10.0.0.1", "a", true), ("10.0.0.2", "b", true)]);
        assert!(changes.observe(&added, Follows::ReadyNodes));
        let unready = slice(&[("10.0.0.1", "a", true), ("10.0.0.2", "b", false)]);
        assert!(changes.observe(&unready, Follows::ReadyNodes));
    }

    // Pod-based targets ignore readiness, so a flap costs nothing there, while an
    // endpoint leaving the slice, terminating or not, moves a target.
    #[test]
    fn pod_targets_ignore_readiness_and_follow_endpoints_leaving() {
        let both = slice(&[("10.0.0.1", "a", true), ("10.0.0.2", "b", true)]);
        let changes = tracked(&both, Follows::AllNodes);
        let unready = slice(&[("10.0.0.1", "a", true), ("10.0.0.2", "b", false)]);
        assert!(!changes.observe(&unready, Follows::AllNodes));
        assert!(changes.observe(&slice(&[("10.0.0.1", "a", true)]), Follows::AllNodes));
    }

    #[test]
    fn deleting_a_slice_with_endpoints_triggers_a_reconcile_of_everything() {
        let initial = slice(&[("10.0.0.1", "a", true)]);
        let changes = tracked(&initial, Follows::ReadyNodes);
        assert!(changes.track(&Event::Delete(initial.clone())));
        // The mapper may see the deletion after the watch did; the slice is gone by
        // then, so it reconciles the service rather than recording the slice again.
        assert!(changes.observe(&initial, Follows::ReadyNodes));
        assert!(!changes.track(&Event::Delete(initial)));
    }

    // A slice deleted while the watch was reconnecting is only noticed by its
    // absence from the new listing, and counts as a deletion.
    #[test]
    fn a_relisting_forgets_slices_that_are_gone() {
        let initial = slice(&[("10.0.0.1", "a", true)]);
        let changes = tracked(&initial, Follows::ReadyNodes);
        assert!(!changes.track(&Event::<EndpointSlice>::Init));
        assert!(changes.track(&Event::<EndpointSlice>::InitDone));
        assert!(changes.observe(&initial, Follows::ReadyNodes));
    }

    // A service that stops using the Local policy stops being observed, and its old
    // record must not hide a change once it uses the policy again.
    #[test]
    fn a_forgotten_slice_counts_as_changed() {
        let initial = slice(&[("10.0.0.1", "a", true)]);
        let changes = tracked(&initial, Follows::ReadyNodes);
        changes.forget(&initial);
        assert!(changes.observe(&initial, Follows::ReadyNodes));
    }

    #[test]
    fn a_relisting_that_drops_only_empty_slices_triggers_nothing() {
        let empty = slice(&[]);
        let changes = tracked(&empty, Follows::ReadyNodes);
        assert!(!changes.track(&Event::<EndpointSlice>::Init));
        assert!(!changes.track(&Event::<EndpointSlice>::InitDone));
    }
}
