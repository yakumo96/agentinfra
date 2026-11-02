//! Backend calls about this daemon's Workspaces; backend Store calls run
//! in the SDK's process (layerfs-store-service).
mod dispatch;
pub(crate) use dispatch::{admits, dispatch};
pub(crate) use infra_version::operator::{snapshot, WorkspaceView};

mod workspace;
