//! `fs.mv`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `mv(from, to, { branch })`: one Commit moving a path.
pub fn mv(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mv")])
}
