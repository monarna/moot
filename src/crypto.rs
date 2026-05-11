use chacha20poly1305::{ChaCha20Poly1305, KeyInit, AeadInPlace, Nonce};
use ed25519_dalek::{VerifyingKey, Signature, Verifier};
use x25519_dalek::{StaticSecret, PublicKey};
use serde::{Serialize, Deserialize};
use hex;
use sha2::{Sha256, Digest};

// Input sanitization

// Garlic Routing Structures
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarlicPacket {
    pub payload: Vec<u8>,
    pub next_hop: Option<String>, // Address of next node
}

#[allow(dead_code)]
pub fn create_garlic_layer(message: &[u8], next_hop: Option<String>, shared_secret: &[u8; 32]) -> Result<Vec<u8>, String> {
    let packet = GarlicPacket {
        payload: message.to_vec(),
        next_hop,
    };
    let serialized = serde_json::to_vec(&packet).map_err(|e| e.to_string())?;
    let (ciphertext, nonce) = encrypt_message(&serialized, shared_secret)?;
    
    // Combine nonce and ciphertext: [nonce (12 bytes)][ciphertext]
    let mut layered = nonce;
    layered.extend(ciphertext);
    Ok(layered)
}

#[allow(dead_code)]
pub fn peel_garlic_layer(layered: &[u8], shared_secret: &[u8; 32]) -> Result<GarlicPacket, String> {
    if layered.len() < 12 {
        return Err("Packet too short".to_string());
    }
    let (nonce_bytes, ciphertext) = layered.split_at(12);
    let plaintext = decrypt_message(ciphertext, shared_secret, nonce_bytes)?;
    serde_json::from_slice(&plaintext).map_err(|e| e.to_string())
}

// Key Exchange
#[allow(dead_code)]
pub fn generate_x25519_keypair() -> (StaticSecret, PublicKey) {
    let secret = StaticSecret::random_from_rng(rand::thread_rng());
    let public = PublicKey::from(&secret);
    (secret, public)
}

#[allow(dead_code)]
pub fn derive_shared_secret(my_secret: &StaticSecret, their_public: &PublicKey) -> [u8; 32] {
    *my_secret.diffie_hellman(their_public).as_bytes()
}

// Input sanitization
pub fn sanitize_content(content: &str) -> String {
    let mut sanitized = String::new();
    let mut in_tag = false;
    for c in content.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => sanitized.push(c),
            _ => {}
        }
    }
    if sanitized.len() > 10000 { sanitized.truncate(10000); }
    sanitized.trim().to_string()
}

pub fn validate_address(address: &str) -> bool {
    address.starts_with("0x") && address.len() == 42 && address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn validate_signature_format(sig: &str) -> bool {
    // Ed25519 signatures are 64 bytes -> 128 hex chars
    sig.len() == 128 && sig.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn sha256_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn verify_signature(public_key_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    let pk_bytes = match hex::decode(public_key_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    
    let sig_bytes = match hex::decode(signature_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    
    if pk_bytes.len() != 32 || sig_bytes.len() != 64 {
        return false;
    }
    
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    
    let verifying_key = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(k) => k,
        Err(_) => return false,
    };
    
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    
    verifying_key.verify(message, &signature).is_ok()
}

// Simple encryption for P2P messages
#[allow(dead_code)]
pub fn encrypt_message(message: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("Cipher init failed: {:?}", e))?;
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut ciphertext = message.to_vec();
    cipher.encrypt_in_place(nonce, b"", &mut ciphertext)
        .map_err(|e| format!("Encryption failed: {:?}", e))?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

#[allow(dead_code)]
pub fn decrypt_message(ciphertext: &[u8], key: &[u8; 32], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("Cipher init failed: {:?}", e))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let mut plaintext = ciphertext.to_vec();
    cipher.decrypt_in_place(nonce, b"", &mut plaintext)
        .map_err(|e| format!("Decryption failed: {:?}", e))?;
    Ok(plaintext)
}
