//! `fs.mkdir`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `mkdir(path, { branch })`: one Commit creating a directory and its missing parents.
pub fn mkdir(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mkdir")])
}
