//! `fs.edit`.
use crate::base::args::{commit, single, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `edit(path, { oldText, newText }, { branch })`: one Commit replacing the one occurrence of a text in a file.
pub fn edit(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "edit")])
}
