use k8s_openapi::{
    api::core::v1::Service,
    serde_json::{json, Value},
};
use kube::{
    api::{Patch, PatchParams},
    Api, Client, ResourceExt,
};

use crate::{
    consts,
    error::{RobotLBError, RobotLBResult},
};

/// Add finalizer to the service.
/// This will prevent the service from being deleted.
pub async fn add(client: Client, svc: &Service) -> RobotLBResult<()> {
    let api = Api::<Service>::namespaced(
        client,
        svc.namespace().ok_or(RobotLBError::SkipService)?.as_str(),
    );
    api.patch(
        svc.name_any().as_str(),
        &PatchParams::default(),
        &add_patch(),
    )
    .await?;
    Ok(())
}

/// Check if service has the finalizer.
#[must_use]
pub fn check(service: &Service) -> bool {
    service
        .metadata
        .finalizers
        .as_ref()
        .map_or(false, |finalizers| {
            finalizers.contains(&consts::FINALIZER_NAME.to_string())
        })
}

/// Remove finalizer from the service.
/// This will allow the service to be deleted.
///
/// if service does not have the finalizer, this function will do nothing.
pub async fn remove(client: Client, svc: &Service) -> RobotLBResult<()> {
    let api = Api::<Service>::namespaced(
        client,
        svc.namespace().ok_or(RobotLBError::SkipService)?.as_str(),
    );
    api.patch(
        svc.name_any().as_str(),
        &PatchParams::default(),
        &remove_patch(),
    )
    .await?;
    Ok(())
}

// `metadata.finalizers` is merged by value in a strategic merge patch, so these
// touch only robotlb's own entry, whatever other controllers add or remove meanwhile.
fn add_patch() -> Patch<Value> {
    Patch::Strategic(json!({
        "metadata": {
            "finalizers": [consts::FINALIZER_NAME]
        }
    }))
}

fn remove_patch() -> Patch<Value> {
    Patch::Strategic(json!({
        "metadata": {
            "$deleteFromPrimitiveList/finalizers": [consts::FINALIZER_NAME]
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{add_patch, remove_patch};
    use crate::consts;
    use k8s_openapi::serde_json::json;
    use kube::api::Patch;

    #[test]
    fn adding_merges_into_the_existing_list() {
        assert_eq!(
            add_patch(),
            Patch::Strategic(json!({"metadata": {"finalizers": [consts::FINALIZER_NAME]}}))
        );
    }

    #[test]
    fn removing_deletes_only_our_finalizer() {
        assert_eq!(
            remove_patch(),
            Patch::Strategic(json!({
                "metadata": {"$deleteFromPrimitiveList/finalizers": [consts::FINALIZER_NAME]}
            }))
        );
    }
}
