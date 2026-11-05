//! Application-layer identity, signatures and encryption for `social-mail`.
//!
//! # Scope and honest labelling
//!
//! v0.1-alpha uses a modern, small, auditable construction rather than OpenPGP:
//!
//! * identity = Ed25519 signing key + X25519 key agreement key
//! * fingerprint = first 20 bytes of `SHA-256(ed25519_pub || x25519_pub)`,
//!   rendered as 40 uppercase hex characters (the same *shape* as an OpenPGP
//!   v4 fingerprint, so the UI and the trust ceremony are unchanged)
//! * signature = Ed25519 detached signature over RFC 8785 canonical JSON
//! * encryption = ephemeral X25519 + SHA-256 KDF + ChaCha20-Poly1305
//! * backup = PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305
//!
//! OpenPGP (the PLAN.md requirement) is **not** implemented yet; naive
//! `rpgp` integration could not be verified in the build environment used for
//! this phase, and shipping an unverified OpenPGP path would be worse than
//! shipping a verified non-OpenPGP one. The public API is shaped so the
//! fingerprint, signature and envelope semantics do not change when the
//! OpenPGP backend lands (`docs/decisions/0002-crypto-backend.md`).

pub mod base64;
mod error;
pub mod hex;
mod identity;

pub use error::CryptoError;
pub use identity::{Identity, PublicIdentity, Signature};

/// Number of hex characters in a fingerprint.
pub const FINGERPRINT_HEX_LEN: usize = 40;

/// Group a 40-character fingerprint into readable blocks of four.
pub fn group_fingerprint(fingerprint: &str) -> String {
    let mut groups: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in fingerprint.chars() {
        current.push(ch);
        if current.chars().count() == 4 {
            groups.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let mut out = String::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            // A wider gap every five groups mirrors common PGP display habits.
            if index == 5 {
                out.push_str("  ");
            } else {
                out.push(' ');
            }
        }
        out.push_str(group);
    }
    out
}

/// Last eight hex characters, used only as a scan aid in lists.
pub fn short_fingerprint(fingerprint: &str) -> String {
    let skip = fingerprint.len().saturating_sub(8);
    fingerprint.chars().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_is_stable() {
        let fp = "3F2A91C48D770B5E6C1D44A09E3877B25A6F0C21";
        assert_eq!(fp.len(), FINGERPRINT_HEX_LEN);
        assert_eq!(
            group_fingerprint(fp),
            "3F2A 91C4 8D77 0B5E 6C1D  44A0 9E38 77B2 5A6F 0C21"
        );
        assert_eq!(short_fingerprint(fp), "5A6F0C21");
    }

    #[test]
    fn short_fingerprint_handles_short_input() {
        assert_eq!(short_fingerprint("ABCD"), "ABCD");
    }
}
