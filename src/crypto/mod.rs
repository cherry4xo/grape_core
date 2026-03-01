use anyhow::{Context, Result};
use libp2p::identity::Keypair;
use libp2p::PeerId;
use rand_core::OsRng;
use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM};
use x25519_dalek::{PublicKey, StaticSecret};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct CryptoError(String);

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Crypto error: {}", self.0)
    }
}

impl Error for CryptoError {}

struct CounterNonceSequence {
    counter: u64,
}

impl CounterNonceSequence {
    fn new() -> Self {
        Self { counter: 0 }
    }
}

impl NonceSequence for CounterNonceSequence {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

pub struct MessageEncryption {
    _local_keypair: Keypair,
    my_x25519_secret_bytes: [u8; 32],
    my_x25519_public: PublicKey,
    peer_shared_secrets: HashMap<PeerId, Vec<u8>>,
}

impl MessageEncryption {
    pub fn new(local_keypair: Keypair) -> Self {
        let my_x25519_secret = StaticSecret::random_from_rng(&mut OsRng);
        let my_x25519_secret_bytes = my_x25519_secret.to_bytes();
        let my_x25519_public = PublicKey::from(&my_x25519_secret);

        Self {
            _local_keypair: local_keypair,
            my_x25519_secret_bytes,
            my_x25519_public,
            peer_shared_secrets: HashMap::new(),
        }
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.my_x25519_public.as_bytes().to_vec()
    }

    pub fn add_peer(&mut self, peer_id: PeerId, peer_public_key: &[u8]) -> Result<()> {
        if peer_public_key.len() != 32 {
            return Err(anyhow::anyhow!("Invalid public key length"));
        }

        let mut peer_public_bytes = [0u8; 32];
        peer_public_bytes.copy_from_slice(peer_public_key);
        let peer_public = PublicKey::from(peer_public_bytes);

        let my_secret = StaticSecret::from(self.my_x25519_secret_bytes);
        let shared_secret = my_secret.diffie_hellman(&peer_public);

        self.peer_shared_secrets.insert(peer_id, shared_secret.as_bytes().to_vec());
        Ok(())
    }

    pub fn encrypt(&self, peer_id: &PeerId, data: &[u8]) -> Result<Vec<u8>> {
        let shared_secret = self.peer_shared_secrets
            .get(peer_id)
            .context("No shared secret for peer")?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &shared_secret[..32])
            .map_err(|e| anyhow::anyhow!("Failed to create key: {:?}", e))?;

        let mut sealing_key = SealingKey::new(unbound_key, CounterNonceSequence::new());

        let mut in_out = data.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

        Ok(in_out)
    }

    pub fn decrypt(&self, peer_id: &PeerId, data: &[u8]) -> Result<Vec<u8>> {
        let shared_secret = self.peer_shared_secrets
            .get(peer_id)
            .context("No shared secret for peer")?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &shared_secret[..32])
            .map_err(|e| anyhow::anyhow!("Failed to create key: {:?}", e))?;

        let mut opening_key = OpeningKey::new(unbound_key, CounterNonceSequence::new());

        let mut in_out = data.to_vec();
        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption() {
        let keypair1 = Keypair::generate_ed25519();
        let keypair2 = Keypair::generate_ed25519();

        let peer_id1 = PeerId::from(keypair1.public());
        let peer_id2 = PeerId::from(keypair2.public());

        let mut crypto1 = MessageEncryption::new(keypair1.clone());
        let mut crypto2 = MessageEncryption::new(keypair2.clone());

        let pub1 = crypto1.public_key();
        let pub2 = crypto2.public_key();

        crypto1.add_peer(peer_id2, &pub2).unwrap();
        crypto2.add_peer(peer_id1, &pub1).unwrap();

        let message = b"Hello, encrypted world!";
        let encrypted = crypto1.encrypt(&peer_id2, message).unwrap();
        let decrypted = crypto2.decrypt(&peer_id1, &encrypted).unwrap();

        assert_eq!(message, decrypted.as_slice());
    }
}
