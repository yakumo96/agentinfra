//! `fs.glob`.
use crate::base::args::{optional_path, paging, snapshot, text, Answer};
use infra_bridge::control::SdkValue;
use infra_version::{read::reads, StoreService};

/// `glob(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: paths matching a `/.lww`-syntax pattern.
pub fn glob(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let pattern = text(args, "pattern")?;
    let path = optional_path(args)?;
    let (limit, pos) = paging(args)?;
    reads::glob(&reader, &snap, pattern, &path, limit, pos)
}
