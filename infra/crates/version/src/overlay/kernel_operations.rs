//! Direct kernel visits and ownership changes on the Workspace connection.
use crate::{OwnerClient, OwnerError, ServiceClass};
use infra_overlay::{KernelCookieOffer, KernelMount};
use infra_workspace::Route;
use infra_workspace::{
    KernelDataVisit, KernelDirectoryVisit, KernelMutationVisit, KernelReadVisit,
};
use std::sync::Arc;
impl OwnerClient {
    pub fn kernel_mount(
        &self,
        route: Route,
        root: u64,
    ) -> Result<infra_overlay::KernelMount, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.create_kernel_mount(root)
        })
    }
    pub fn kernel_close_file(
        &self,
        route: Route,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> Result<(), OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.kernel_close_file(mount, serial, handle)
        })
    }
    pub fn kernel_observe_visit(
        &self,
        route: Route,
        job: Box<KernelReadVisit>,
    ) -> Result<std::sync::Arc<infra_workspace::KernelReadOutcome>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            Ok(Arc::new(job.perform(db)))
        })
    }
    pub fn kernel_read_visit(
        &self,
        route: Route,
        job: Box<KernelDataVisit>,
    ) -> Result<std::sync::Arc<infra_workspace::KernelWindow>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            job.perform(db).map(Arc::new)
        })
    }
    pub fn kernel_mutate_visit(
        &self,
        route: Route,
        job: Box<KernelMutationVisit>,
    ) -> Result<std::sync::Arc<infra_workspace::KernelMutationOutcome>, OwnerError> {
        self.call(route, ServiceClass::Mutation, None, move |db| {
            Ok(Arc::new(job.perform(db)))
        })
    }
    pub fn kernel_forget(
        &self,
        route: Route,
        mount: KernelMount,
        serial: u64,
        count: u64,
    ) -> Result<(), OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.kernel_forget(mount, serial, count)
        })
    }
    pub fn kernel_retained_mount(
        &self,
        route: Route,
    ) -> Result<Option<infra_overlay::KernelMount>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.retained_kernel_mount()
        })
    }
    pub fn kernel_state(
        &self,
        route: Route,
        mount: KernelMount,
    ) -> Result<infra_overlay::KernelMountState, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.kernel_mount_state(mount)
        })
    }
    pub fn kernel_revoke(&self, route: Route, mount: KernelMount) -> Result<(), OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.revoke_kernel_mount(mount)
        })
    }
    pub fn kernel_directory_visit(
        &self,
        route: Route,
        visit: KernelDirectoryVisit,
    ) -> Result<std::sync::Arc<infra_workspace::KernelDirectoryWindow>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            visit.perform(db).map(Arc::new)
        })
    }
    pub fn kernel_directory_publish(
        &self,
        route: Route,
        offer: KernelCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> Result<(), OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.publish_kernel_cookies(&offer, &names)
        })
    }
    pub fn kernel_directory_close(
        &self,
        route: Route,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> Result<(), OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.close_kernel_directory(mount, serial, handle)
        })
    }
}
