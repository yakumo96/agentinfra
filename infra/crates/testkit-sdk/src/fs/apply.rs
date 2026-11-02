//! `fs.apply`.
use crate::base::args::{commit, invalid, Answer};
use infra_bridge::control::SdkValue;
use infra_version::StoreService;

/// `apply(changes[], { branch })`: one Commit for every change.
pub fn apply(service: &StoreService, args: &SdkValue) -> Answer {
    let Some(SdkValue::List(changes)) = args.get("changes") else {
        return Err(invalid("apply requires a changes array"));
    };
    commit(service, args, changes)
}
