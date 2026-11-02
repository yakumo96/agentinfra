//! `fs.write`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `write(path, content, { branch })`: one Commit writing a file.
pub fn write(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "write")])
}
