use anyhow::{Result, anyhow};
use ring::{
    rand::SystemRandom,
    signature::{ED25519, Ed25519KeyPair, KeyPair},
};
use std::fs;
use std::path::Path;

/// Generates a new Ed25519 key pair and returns (private_key_bytes, public_key_bytes).
/// Note: ring's generate_pkcs8 returns the document, not the keypair directly,
/// so we must parse it a second time to extract the public key.
pub fn generate_keypair() -> Result<(Vec<u8>, Vec<u8>)> {
    let rng = SystemRandom::new();
    let private_key_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow!("Failed to generate key pair"))?;

    let private_key = Ed25519KeyPair::from_pkcs8(private_key_bytes.as_ref())
        .map_err(|_| anyhow!("Failed to create key pair from bytes"))?;

    let public_key_bytes = private_key.public_key().as_ref().to_vec();

    Ok((private_key_bytes.as_ref().to_vec(), public_key_bytes))
}

/// Derives a node ID from a public key by hashing it.
/// This implements self-sovereign identity: the node ID is a cryptographic function of the public key.
pub fn derive_node_id(public_key_bytes: &[u8]) -> u64 {
    let hash = blake3::hash(public_key_bytes);
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
}

/// Parses a private key from PKCS8 bytes.
pub fn load_keypair(private_key_bytes: &[u8]) -> Result<Ed25519KeyPair> {
    Ed25519KeyPair::from_pkcs8(private_key_bytes)
        .map_err(|_| anyhow!("Failed to load private key"))
}

/// Signs a message with the parsed keypair (no re-parsing)
pub fn sign(keypair: &Ed25519KeyPair, message: &[u8]) -> Vec<u8> {
    keypair.sign(message).as_ref().to_vec()
}

/// Constructs the canonical buffer to sign: msg_id || sender_id || payload
pub fn canonical_message(msg_id: u64, sender_id: u64, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + payload.len());
    buf.extend_from_slice(&msg_id.to_le_bytes());
    buf.extend_from_slice(&sender_id.to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Verifies a signature with the public key
pub fn verify(public_key_bytes: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    let untrusted_pub_key = ring::signature::UnparsedPublicKey::new(&ED25519, public_key_bytes);
    untrusted_pub_key
        .verify(message, signature)
        .map_err(|_| anyhow!("Signature verification failed"))
}

/// Loads a private key from a file
pub fn load_private_key<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| anyhow!("Failed to read private key: {}", e))
}

/// Saves a private key to a file
pub fn save_private_key<P: AsRef<Path>>(path: P, key: &[u8]) -> Result<()> {
    fs::write(path, key).map_err(|e| anyhow!("Failed to save private key: {}", e))
}
