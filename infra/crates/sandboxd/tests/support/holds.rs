//! Workspace route selection for mounted FORGET/drain proofs.
use infra_bridge::control::WorkspaceToken;
use infra_sandboxd::control::Service;
use infra_workspace::Route;
pub fn route(service: &Service, token: WorkspaceToken) -> Route {
    service.operation(token).unwrap().workspace().route()
}
