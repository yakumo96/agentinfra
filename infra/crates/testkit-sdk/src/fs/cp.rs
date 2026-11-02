//! `fs.cp`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `cp(from, to, { branch })`: one Commit copying a path, recursively.
pub fn cp(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "cp")])
}
