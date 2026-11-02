//! `branch.list_commits`.
use crate::base::args::{paging, project, text, Answer};
use infra_bridge::control::SdkValue;
use infra_version::{
    history::{branch, lookup},
    StoreService,
};

/// `list_commits({ branch, pageSize?, pagePos? })`: one page, newest first.
pub fn list_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    let (limit, pos) = paging(args)?;
    branch::list_commits(&service.store, &named, limit, pos)
}
