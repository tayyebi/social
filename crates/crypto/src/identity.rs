//! Identity keys, detached signatures, message encryption and backups.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::{base64, hex, CryptoError, FINGERPRINT_HEX_LEN};

/// Magic prefix of an encrypted application payload.
const ENC_PREFIX: &[u8; 6] = b"SMENC1";
const KDF_LABEL: &[u8] = b"social-mail/v1/enc";
const BACKUP_KIND: &str = "social-mail-identity-backup";
const BACKUP_VERSION: u32 = 1;

/// PBKDF2 rounds used for identity backups.
pub const DEFAULT_KDF_ROUNDS: u32 = 200_000;

/// A local identity: a signing key and a key-agreement key.
#[derive(Clone)]
pub struct Identity {
    signing: SigningKey,
    encryption: StaticSecret,
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print secret material, not even in debug dumps.
        write!(f, "Identity({})", self.fingerprint())
    }
}

impl Identity {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let encryption = StaticSecret::random_from_rng(OsRng);
        Identity {
            signing,
            encryption,
        }
    }

    /// Rebuild an identity from raw secret bytes (32-byte Ed25519 seed and
    /// 32-byte X25519 secret).
    pub fn from_secrets(seed: [u8; 32], encryption_secret: [u8; 32]) -> Self {
        Identity {
            signing: SigningKey::from_bytes(&seed),
            encryption: StaticSecret::from(encryption_secret),
        }
    }

    pub fn secrets(&self) -> ([u8; 32], [u8; 32]) {
        (self.signing.to_bytes(), self.encryption.to_bytes())
    }

    pub fn public(&self) -> PublicIdentity {
        PublicIdentity {
            ed25519: self.signing.verifying_key().to_bytes(),
            x25519: X25519PublicKey::from(&self.encryption).to_bytes(),
        }
    }

    pub fn fingerprint(&self) -> String {
        self.public().fingerprint()
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature(self.signing.sign(message).to_bytes())
    }

    /// Decrypt a payload produced by [`PublicIdentity::encrypt_to`] for this
    /// identity.
    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
        const HEADER: usize = 6 + 32 + 12;
        if blob.len() < HEADER + 16 {
            return Err(CryptoError::Decrypt);
        }
        if &blob[..6] != ENC_PREFIX {
            return Err(CryptoError::Decrypt);
        }
        let mut ephemeral = [0u8; 32];
        ephemeral.copy_from_slice(&blob[6..38]);
        let nonce_bytes: [u8; 12] = blob[38..50]
            .try_into()
            .map_err(|_| CryptoError::Decrypt)?;
        let recipient = X25519PublicKey::from(&self.encryption);
        let shared = self.encryption.diffie_hellman(&X25519PublicKey::from(ephemeral));
        let key = derive_key(shared.as_bytes(), &ephemeral, recipient.as_bytes());
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| CryptoError::Key("bad cipher key length".into()))?;
        cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), &blob[HEADER..])
            .map_err(|_| CryptoError::Decrypt)
    }

    pub fn export_backup(&self, passphrase: &str) -> Result<String, CryptoError> {
        self.export_backup_with_rounds(passphrase, DEFAULT_KDF_ROUNDS)
    }

    pub fn export_backup_with_rounds(
        &self,
        passphrase: &str,
        rounds: u32,
    ) -> Result<String, CryptoError> {
        if passphrase.is_empty() {
            return Err(CryptoError::Backup(
                "a backup needs a non-empty passphrase".into(),
            ));
        }
        let (seed, encryption_secret) = self.secrets();
        let plaintext = serde_json::to_vec(&SecretMaterial {
            ed25519_seed: base64::encode(&seed),
            x25519_secret: base64::encode(&encryption_secret),
        })?;

        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let key = derive_passphrase_key(passphrase, &salt, rounds);
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| CryptoError::Key("bad cipher key length".into()))?;
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_slice())
            .map_err(|_| CryptoError::Decrypt)?;

        let file = BackupFile {
            kind: BACKUP_KIND.to_string(),
            version: BACKUP_VERSION,
            fingerprint: self.fingerprint(),
            kdf: KdfParams {
                name: "pbkdf2-hmac-sha256".to_string(),
                rounds,
                salt: base64::encode(&salt),
            },
            cipher: "chacha20poly1305".to_string(),
            nonce: base64::encode(&nonce_bytes),
            ciphertext: base64::encode(&ciphertext),
        };
        Ok(serde_json::to_string_pretty(&file)?)
    }

    pub fn import_backup(text: &str, passphrase: &str) -> Result<(Identity, String), CryptoError> {
        let file: BackupFile = serde_json::from_str(text)?;
        if file.kind != BACKUP_KIND {
            return Err(CryptoError::Backup(format!(
                "unexpected backup kind {:?}",
                file.kind
            )));
        }
        if file.version != BACKUP_VERSION {
            return Err(CryptoError::Backup(format!(
                "unsupported backup version {}",
                file.version
            )));
        }
        if file.kdf.name != "pbkdf2-hmac-sha256" || file.cipher != "chacha20poly1305" {
            return Err(CryptoError::Backup(
                "unsupported kdf or cipher in backup".into(),
            ));
        }
        if file.kdf.rounds == 0 || file.kdf.rounds > 10_000_000 {
            return Err(CryptoError::Backup("implausible kdf rounds".into()));
        }
        let salt = base64::decode(&file.kdf.salt)?;
        let nonce_bytes = base64::decode(&file.nonce)?;
        if nonce_bytes.len() != 12 || salt.len() < 8 {
            return Err(CryptoError::Backup("malformed nonce or salt".into()));
        }
        let ciphertext = base64::decode(&file.ciphertext)?;
        let key = derive_passphrase_key(passphrase, &salt, file.kdf.rounds);
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| CryptoError::Key("bad cipher key length".into()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_slice())
            .map_err(|_| CryptoError::Passphrase)?;
        let material: SecretMaterial = serde_json::from_slice(&plaintext)?;
        let seed = fixed_32(&base64::decode(&material.ed25519_seed)?)?;
        let encryption_secret = fixed_32(&base64::decode(&material.x25519_secret)?)?;
        let identity = Identity::from_secrets(seed, encryption_secret);
        Ok((identity, file.fingerprint))
    }
}

fn fixed_32(bytes: &[u8]) -> Result<[u8; 32], CryptoError> {
    if bytes.len() != 32 {
        return Err(CryptoError::Key("key material is not 32 bytes".into()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn derive_key(shared: &[u8; 32], ephemeral: &[u8; 32], recipient: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(KDF_LABEL);
    hasher.update(shared);
    hasher.update(ephemeral);
    hasher.update(recipient);
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn derive_passphrase_key(passphrase: &str, salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, rounds, &mut key);
    key
}

#[derive(Serialize, Deserialize)]
struct BackupFile {
    kind: String,
    version: u32,
    fingerprint: String,
    kdf: KdfParams,
    cipher: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct KdfParams {
    name: String,
    rounds: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
struct SecretMaterial {
    ed25519_seed: String,
    x25519_secret: String,
}

/// A detached Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; 64]);

impl Signature {
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Signature(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::to_hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Result<Self, CryptoError> {
        let bytes = hex::from_hex(text)?;
        if bytes.len() != 64 {
            return Err(CryptoError::Key("signature is not 64 bytes".into()));
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&bytes);
        Ok(Signature(out))
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({}…)", &self.to_hex()[..16])
    }
}

/// A public identity, as published in contact cards and follow requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicIdentity {
    pub ed25519: [u8; 32],
    pub x25519: [u8; 32],
}

impl PublicIdentity {
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.ed25519);
        hasher.update(self.x25519);
        let digest = hasher.finalize();
        hex::to_hex(&digest[..FINGERPRINT_HEX_LEN / 2]).to_uppercase()
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let Ok(key) = VerifyingKey::from_bytes(&self.ed25519) else {
            return false;
        };
        let sig = Ed25519Signature::from_bytes(signature.as_bytes());
        key.verify_strict(message, &sig).is_ok()
    }

    pub fn encrypt_to(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let ephemeral = StaticSecret::random_from_rng(OsRng);
        let ephemeral_public = X25519PublicKey::from(&ephemeral);
        let recipient = X25519PublicKey::from(self.x25519);
        let shared = ephemeral.diffie_hellman(&recipient);
        let key = derive_key(shared.as_bytes(), ephemeral_public.as_bytes(), &self.x25519);
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| CryptoError::Key("bad cipher key length".into()))?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| CryptoError::Decrypt)?;
        let mut out = Vec::with_capacity(6 + 32 + 12 + ciphertext.len());
        out.extend_from_slice(ENC_PREFIX);
        out.extend_from_slice(ephemeral_public.as_bytes());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn to_b64(&self) -> (String, String) {
        (base64::encode(&self.ed25519), base64::encode(&self.x25519))
    }

    pub fn from_b64(ed25519: &str, x25519: &str) -> Result<Self, CryptoError> {
        Ok(PublicIdentity {
            ed25519: fixed_32(&base64::decode(ed25519)?)?,
            x25519: fixed_32(&base64::decode(x25519)?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_backup(identity: &Identity, passphrase: &str) -> String {
        identity
            .export_backup_with_rounds(passphrase, 1_000)
            .expect("backup")
    }

    #[test]
    fn fingerprint_shape_is_stable() {
        let identity = Identity::from_secrets([7u8; 32], [9u8; 32]);
        let fingerprint = identity.fingerprint();
        assert_eq!(fingerprint.len(), FINGERPRINT_HEX_LEN);
        assert!(fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        // Rebuilding the same secrets yields the same identity.
        let again = Identity::from_secrets([7u8; 32], [9u8; 32]);
        assert_eq!(again.fingerprint(), fingerprint);
        // Changing one key changes the fingerprint.
        let other = Identity::from_secrets([8u8; 32], [9u8; 32]);
        assert_ne!(other.fingerprint(), fingerprint);
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let identity = Identity::generate();
        let message = b"canonical event bytes";
        let signature = identity.sign(message);
        assert!(identity.public().verify(message, &signature));
        assert!(!identity.public().verify(b"other bytes", &signature));

        let mut tampered = signature.as_bytes().to_vec();
        tampered[0] ^= 0xff;
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&tampered);
        assert!(!identity
            .public()
            .verify(message, &Signature::from_bytes(bytes)));
    }

    #[test]
    fn signature_hex_round_trip() {
        let identity = Identity::generate();
        let signature = identity.sign(b"payload");
        let parsed = Signature::from_hex(&signature.to_hex()).unwrap();
        assert_eq!(parsed, signature);
        assert!(Signature::from_hex("00").is_err());
    }

    #[test]
    fn public_identity_base64_round_trip() {
        let identity = Identity::generate();
        let public = identity.public();
        let (ed, x) = public.to_b64();
        assert_eq!(PublicIdentity::from_b64(&ed, &x).unwrap(), public);
        assert!(PublicIdentity::from_b64(&ed, "AAAA").is_err());
    }

    #[test]
    fn encryption_round_trip_and_wrong_recipient() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let eve = Identity::generate();
        let plaintext = "سلام دنیا — secret".as_bytes();
        let blob = bob.public().encrypt_to(plaintext).unwrap();
        assert_eq!(bob.decrypt(&blob).unwrap(), plaintext);
        assert!(alice.decrypt(&blob).is_err());
        assert!(eve.decrypt(&blob).is_err());
        // Tampering is detected by the AEAD tag.
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(bob.decrypt(&tampered).is_err());
        // Garbage is rejected without panicking.
        assert!(bob.decrypt(b"not a blob").is_err());
        assert!(bob.decrypt(&[]).is_err());
    }

    #[test]
    fn encryption_is_randomised() {
        let bob = Identity::generate();
        let first = bob.public().encrypt_to(b"same text").unwrap();
        let second = bob.public().encrypt_to(b"same text").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn backup_round_trip_and_wrong_passphrase() {
        let identity = Identity::generate();
        let backup = fast_backup(&identity, "correct horse battery staple");
        let (restored, fingerprint) =
            Identity::import_backup(&backup, "correct horse battery staple").unwrap();
        assert_eq!(fingerprint, identity.fingerprint());
        assert_eq!(restored.secrets(), identity.secrets());
        // Signature made by the restored key verifies against the original.
        let signature = restored.sign(b"continuity");
        assert!(identity.public().verify(b"continuity", &signature));

        assert!(matches!(
            Identity::import_backup(&backup, "wrong passphrase"),
            Err(CryptoError::Passphrase)
        ));
        assert!(matches!(
            Identity::import_backup("{}", "x"),
            Err(CryptoError::Serde(_))
        ));
    }

    #[test]
    fn backup_requires_passphrase() {
        let identity = Identity::generate();
        assert!(identity.export_backup("").is_err());
    }

    #[test]
    fn backup_rejects_tampered_ciphertext() {
        let identity = Identity::generate();
        let backup = fast_backup(&identity, "passphrase");
        let mut value: serde_json::Value = serde_json::from_str(&backup).unwrap();
        let ciphertext = value["ciphertext"].as_str().unwrap().to_string();
        let mut bytes = base64::decode(&ciphertext).unwrap();
        bytes[0] ^= 0xff;
        value["ciphertext"] = serde_json::Value::String(base64::encode(&bytes));
        let tampered = serde_json::to_string(&value).unwrap();
        assert!(Identity::import_backup(&tampered, "passphrase").is_err());
    }

    #[test]
    fn debug_output_hides_secrets() {
        let identity = Identity::from_secrets([3u8; 32], [4u8; 32]);
        let rendered = format!("{identity:?}");
        assert!(rendered.contains("Identity("));
        assert!(!rendered.contains("03030303"));
    }
}
