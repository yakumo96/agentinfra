//! Typed ID prefixes from `schemas/common/defs.json`.
//!
//! ULID-based IDs (`ws_<ulid>`) are minted here; digest-based IDs (`cm_<blake3>`)
//! are computed by the version engine and only validated here.

use std::fs::File;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UlidKind {
    Revision,
    Branch,
    Workspace,
    Checkpoint,
    Run,
    WorkItem,
    Proposal,
    Actor,
    Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestKind {
    Commit,
    Root,
    Artifact,
}

impl UlidKind {
    pub const fn prefix(self) -> &'static str {
        match self {
            UlidKind::Revision => "rev",
            UlidKind::Branch => "br",
            UlidKind::Workspace => "ws",
            UlidKind::Checkpoint => "ck",
            UlidKind::Run => "run",
            UlidKind::WorkItem => "wi",
            UlidKind::Proposal => "pr",
            UlidKind::Actor => "act",
            UlidKind::Operation => "op",
        }
    }
}

impl DigestKind {
    pub const fn prefix(self) -> &'static str {
        match self {
            DigestKind::Commit => "cm",
            DigestKind::Root => "root",
            DigestKind::Artifact => "art",
        }
    }
}

/// Mint a new `<prefix>_<ulid>` ID.
pub fn mint(kind: UlidKind) -> std::io::Result<String> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let mut rnd = [0u8; 10];
    File::open("/dev/urandom")?.read_exact(&mut rnd)?;
    Ok(format!("{}_{}", kind.prefix(), encode_ulid(ms, rnd)))
}

/// Crockford base32 of 48-bit milliseconds followed by 80 random bits (26 chars).
pub fn encode_ulid(ms: u64, rnd: [u8; 10]) -> String {
    let mut value: u128 = u128::from(ms & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in rnd.iter().enumerate() {
        value |= u128::from(*b) << (72 - 8 * i);
    }
    let mut out = [0u8; 26];
    for (i, slot) in out.iter_mut().enumerate() {
        let shift = 125 - 5 * i as u32;
        *slot = CROCKFORD[((value >> shift) & 0x1F) as usize];
    }
    String::from_utf8(out.to_vec()).expect("alphabet is ASCII")
}

pub fn is_ulid_id(kind: UlidKind, id: &str) -> bool {
    match id
        .strip_prefix(kind.prefix())
        .and_then(|r| r.strip_prefix('_'))
    {
        Some(body) => {
            body.len() == 26
                && body.bytes().all(|b| CROCKFORD.contains(&b))
                && body.as_bytes()[0] <= b'7'
        }
        None => false,
    }
}

pub fn digest_id(kind: DigestKind, hex: &str) -> Option<String> {
    is_hex64(hex).then(|| format!("{}_{}", kind.prefix(), hex))
}

pub fn is_digest_id(kind: DigestKind, id: &str) -> bool {
    id.strip_prefix(kind.prefix())
        .and_then(|r| r.strip_prefix('_'))
        .is_some_and(is_hex64)
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
