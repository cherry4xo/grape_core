use libp2p::{identity::Keypair, PeerId};
use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

pub struct Identity {
    keypair: Keypair,
    peer_id: PeerId,
}

impl Identity {
    pub fn generate() -> Self {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        Self { keypair, peer_id }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let encoded = self.keypair.to_protobuf_encoding()
            .context("Failed to encode keypair")?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, encoded)
            .context("Failed to write keypair to file")?;

        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .context("Failed to read keypair file")?;

        let keypair = Keypair::from_protobuf_encoding(&bytes)
            .context("Failed to decode keypair")?;

        let peer_id = PeerId::from(keypair.public());

        Ok(Self { keypair, peer_id })
    }

    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let identity = Self::generate();
            identity.save(path)?;
            Ok(identity)
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    pub fn public_key_str(&self) -> String {
        self.peer_id.to_base58()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_generate_identity() {
        let identity = Identity::generate();
        assert!(!identity.peer_id().to_string().is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = env::temp_dir();
        let path = temp_dir.join("test_identity.key");

        let identity1 = Identity::generate();
        identity1.save(&path).unwrap();

        let identity2 = Identity::load(&path).unwrap();

        assert_eq!(identity1.peer_id(), identity2.peer_id());

        fs::remove_file(path).ok();
    }
}