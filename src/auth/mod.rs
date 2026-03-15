use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString},
    Argon2,
};
use bip39::{Language, Mnemonic};
use libp2p::identity::Keypair;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tiny_hderive::bip32::ExtendedPrivKey;

pub const DERIVATION_PATH: &str = "m/44'/501'/0'/0'";

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedSeed {
    pub ciphertext: Vec<u8>,
    pub salt: String,
    pub nonce: [u8; 12],
}

pub struct AuthManager {
    mnemonic: Option<Mnemonic>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self { mnemonic: None }
    }

    pub fn generate_mnemonic(&mut self) -> Result<String> {
        let mnemonic = Mnemonic::generate_in(Language::English, 12)?;
        let words = mnemonic.to_string();
        self.mnemonic = Some(mnemonic);
        Ok(words)
    }

    pub fn import_mnemonic(&mut self, words: &str) -> Result<()> {
        let mnemonic = Mnemonic::parse_in(Language::English, words)
            .map_err(|e| anyhow!("Invalid mnemonic: {}", e))?;
        self.mnemonic = Some(mnemonic);
        Ok(())
    }

    pub fn validate_mnemonic(words: &str) -> bool {
        Mnemonic::parse_in(Language::English, words).is_ok()
    }

    pub fn derive_keypair(&self) -> Result<Keypair> {
        let mnemonic = self.mnemonic.as_ref()
            .ok_or_else(|| anyhow!("No mnemonic loaded"))?;

        let seed = mnemonic.to_seed("");
        let ext = ExtendedPrivKey::derive(&seed, DERIVATION_PATH)
            .map_err(|e| anyhow!("Key derivation failed: {:?}", e))?;

        let keypair = Keypair::ed25519_from_bytes(ext.secret())
            .map_err(|e| anyhow!("Failed to create keypair: {:?}", e))?;

        Ok(keypair)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P, password: Option<&str>) -> Result<()> {
        let mnemonic = self.mnemonic.as_ref()
            .ok_or_else(|| anyhow!("No mnemonic to save"))?;

        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        if let Some(pwd) = password {
            let encrypted = self.encrypt_seed(mnemonic.to_string().as_bytes(), pwd)?;
            let json = serde_json::to_vec(&encrypted)?;
            fs::write(path, json)?;
        } else {
            let json = serde_json::json!({ "mnemonic": mnemonic.to_string() });
            fs::write(path, json.to_string())?;
        }

        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P, password: Option<&str>) -> Result<Self> {
        let data = fs::read(path)?;

        if let Ok(encrypted) = serde_json::from_slice::<EncryptedSeed>(&data) {
            let pwd = password.ok_or_else(|| anyhow!("Password required"))?;
            let plaintext = Self::decrypt_seed(&encrypted, pwd)?;
            let words = String::from_utf8(plaintext)?;
            let mut manager = Self::new();
            manager.import_mnemonic(&words)?;
            Ok(manager)
        } else {
            let json: serde_json::Value = serde_json::from_slice(&data)?;
            let words = json["mnemonic"].as_str()
                .ok_or_else(|| anyhow!("Invalid seed file format"))?;
            let mut manager = Self::new();
            manager.import_mnemonic(words)?;
            Ok(manager)
        }
    }

    fn encrypt_seed(&self, plaintext: &[u8], password: &str) -> Result<EncryptedSeed> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash_str = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Argon2 error: {}", e))?
            .to_string();

        let parsed = PasswordHash::new(&hash_str)
            .map_err(|e| anyhow!("Failed to parse hash: {}", e))?;
        let hash_output = parsed.hash
            .ok_or_else(|| anyhow!("No hash output in PHC string"))?;
        let hash_bytes = hash_output.as_bytes();
        if hash_bytes.len() < 32 {
            return Err(anyhow!("Hash output too short"));
        }
        let key_bytes = &hash_bytes[..32];

        let unbound = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| anyhow!("Failed to create AES key"))?;
        let key = LessSafeKey::new(unbound);

        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("Failed to generate nonce"))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut ciphertext = plaintext.to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        Ok(EncryptedSeed {
            ciphertext,
            salt: salt.to_string(),
            nonce: nonce_bytes,
        })
    }

    fn decrypt_seed(encrypted: &EncryptedSeed, password: &str) -> Result<Vec<u8>> {
        let argon2 = Argon2::default();
        let salt = SaltString::from_b64(&encrypted.salt)
            .map_err(|e| anyhow!("Invalid salt: {}", e))?;
        let hash_str = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Argon2 error: {}", e))?
            .to_string();

        let parsed = PasswordHash::new(&hash_str)
            .map_err(|e| anyhow!("Failed to parse hash: {}", e))?;
        let hash_output = parsed.hash
            .ok_or_else(|| anyhow!("No hash output in PHC string"))?;
        let hash_bytes = hash_output.as_bytes();
        if hash_bytes.len() < 32 {
            return Err(anyhow!("Hash output too short"));
        }
        let key_bytes = &hash_bytes[..32];

        let unbound = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| anyhow!("Failed to create AES key"))?;
        let key = LessSafeKey::new(unbound);

        let nonce = Nonce::assume_unique_for_key(encrypted.nonce);
        let mut buf = encrypted.ciphertext.clone();
        let plaintext = key
            .open_in_place(nonce, Aad::empty(), &mut buf)
            .map_err(|_| anyhow!("Decryption failed — wrong password?"))?;

        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_derive() {
        let mut auth = AuthManager::new();
        let words = auth.generate_mnemonic().unwrap();
        assert_eq!(words.split_whitespace().count(), 12);
        let keypair = auth.derive_keypair().unwrap();
        let peer_id = keypair.public().to_peer_id();
        println!("Derived PeerId: {}", peer_id);
    }

    #[test]
    fn test_same_seed_same_peer_id() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mut auth1 = AuthManager::new();
        auth1.import_mnemonic(words).unwrap();
        let kp1 = auth1.derive_keypair().unwrap();

        let mut auth2 = AuthManager::new();
        auth2.import_mnemonic(words).unwrap();
        let kp2 = auth2.derive_keypair().unwrap();

        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }

    #[test]
    fn test_encrypted_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.enc");

        let mut auth = AuthManager::new();
        auth.generate_mnemonic().unwrap();
        let kp_before = auth.derive_keypair().unwrap();

        auth.save(&path, Some("test_password")).unwrap();

        let loaded = AuthManager::load(&path, Some("test_password")).unwrap();
        let kp_after = loaded.derive_keypair().unwrap();

        assert_eq!(
            kp_before.public().to_peer_id(),
            kp_after.public().to_peer_id()
        );
    }

    #[test]
    fn test_wrong_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.enc");

        let mut auth = AuthManager::new();
        auth.generate_mnemonic().unwrap();
        auth.save(&path, Some("correct")).unwrap();

        let result = AuthManager::load(&path, Some("wrong"));
        assert!(result.is_err());
    }
}
