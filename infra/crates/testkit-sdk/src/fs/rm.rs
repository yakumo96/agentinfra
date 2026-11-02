//! `fs.rm`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `rm(path, { branch })`: one Commit removing a path, recursively.
pub fn rm(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "rm")])
}
