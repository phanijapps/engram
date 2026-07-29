//! Project → Scope resolution for the fused-per-project model.
//!
//! Every write and recall carries the project scope, so all knowledge for one
//! project shares one searchable space and unrelated projects never blend
//! (spec AC: fused-per-project + scope isolation). Recall-lane helpers land
//! with the recall tool in T7.

use engram_domain::Scope;

/// Resolve a project name + tenant into the project scope.
///
/// `workspace` carries the project; `tenant` carries the host/agent. `subject`
/// (agent identity), `session`, and `environment` are unset in Phase 1.
#[allow(dead_code)] // first consumed by the write/recall tools in T5
pub fn project_scope(project: &str, tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_string(),
        workspace: Some(project.to_string()),
        subject: None,
        session: None,
        environment: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_maps_to_workspace() {
        let scope = project_scope("foo", "default");
        assert_eq!(scope.workspace.as_deref(), Some("foo"));
        assert_eq!(scope.tenant, "default");
        assert!(scope.subject.is_none());
        assert!(scope.session.is_none());
        assert!(scope.environment.is_none());
    }

    #[test]
    fn different_projects_produce_different_workspaces() {
        assert_ne!(project_scope("a", "t"), project_scope("b", "t"));
        assert_eq!(project_scope("a", "t"), project_scope("a", "t"));
    }
}
