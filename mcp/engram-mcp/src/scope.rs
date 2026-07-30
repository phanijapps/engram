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

/// Resolve the fused scope from the launch config (RFC-0016 D2). `org` is the
/// tenant (ownership boundary); the workspace is the `domain` with an optional
/// `/subdomain`, falling back to the legacy `project` name when neither is given
/// so existing `--project` deployments keep working. Strict scope matching
/// isolates subdomains — sibling subdomains cannot blend.
pub fn resolve_scope(
    org: Option<&str>,
    domain: Option<&str>,
    subdomain: Option<&str>,
    project: &str,
) -> Scope {
    let tenant = org.unwrap_or("default").to_owned();
    let workspace = match (domain, subdomain) {
        (Some(d), Some(s)) => format!("{d}/{s}"),
        (Some(d), None) => d.to_owned(),
        _ => project.to_owned(),
    };
    Scope {
        tenant,
        workspace: Some(workspace),
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

    #[test]
    fn resolve_scope_org_domain_subdomain() {
        let s = resolve_scope(Some("acme"), Some("checkout"), Some("payments"), "default");
        assert_eq!(s.tenant, "acme");
        assert_eq!(s.workspace.as_deref(), Some("checkout/payments"));
    }

    #[test]
    fn resolve_scope_domain_without_subdomain() {
        let s = resolve_scope(Some("acme"), Some("checkout"), None, "default");
        assert_eq!(s.tenant, "acme");
        assert_eq!(s.workspace.as_deref(), Some("checkout"));
    }

    #[test]
    fn resolve_scope_project_backward_compat() {
        // No org/domain → legacy --project maps to workspace, tenant defaults.
        let s = resolve_scope(None, None, None, "agentzero");
        assert_eq!(s.tenant, "default");
        assert_eq!(s.workspace.as_deref(), Some("agentzero"));
    }
}
