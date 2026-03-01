use anyhow::{Context, Result};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub chat_id: String,
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
    pub is_outgoing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub peer_id: String,
    pub name: Option<String>,
    pub last_seen: Option<i64>,
}

pub struct Storage {
    db: Db,
}

impl Storage {
    pub fn new(path: &Path) -> Result<Self> {
        let db = sled::open(path)
            .context("Failed to open database")?;

        Ok(Self { db })
    }

    pub fn save_message(&self, message: &StoredMessage) -> Result<()> {
        let messages_tree = self.db.open_tree("messages")?;

        let key = format!("{}_{}_{}",
            message.timestamp,
            message.chat_id,
            message.id,
        );

        let value = bincode::serialize(message)?;
        messages_tree.insert(key.as_bytes(), value)?;

        Ok(())
    }

    pub fn get_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<StoredMessage>> {
        let messages_tree = self.db.open_tree("messages")?;

        let mut messages = Vec::new();
        let prefix = format!("_{}_", chat_id);

        for item in messages_tree.iter().rev() {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.contains(&prefix) {
                let message: StoredMessage = bincode::deserialize(&value)?;
                messages.push(message);

                if messages.len() >= limit {
                    break;
                }
            }
        }

        messages.reverse();
        Ok(messages)
    }

    pub fn save_contact(&self, contact: &Contact) -> Result<()> {
        let contacts_tree = self.db.open_tree("contacts")?;

        let key = contact.peer_id.as_bytes();
        let value = bincode::serialize(contact)?;

        contacts_tree.insert(key, value)?;
        Ok(())
    }

    pub fn get_contact(&self, peer_id: &PeerId) -> Result<Option<Contact>> {
        let contacts_tree = self.db.open_tree("contacts")?;

        let key = peer_id.to_string();
        if let Some(value) = contacts_tree.get(key.as_bytes())? {
            let contact: Contact = bincode::deserialize(&value)?;
            return Ok(Some(contact));
        }

        Ok(None)
    }

    pub fn get_contacts(&self) -> Result<Vec<Contact>> {
        let contacts_tree = self.db.open_tree("contacts")?;

        let mut contacts = Vec::new();
        for item in contacts_tree.iter() {
            let (_, value) = item?;
            let contact: Contact = bincode::deserialize(&value)?;
            contacts.push(contact);
        }

        Ok(contacts)
    }

    pub fn remove_contact(&self, peer_id: &PeerId) -> Result<()> {
        let contacts_tree = self.db.open_tree("contacts")?;
        let key = peer_id.to_string();
        contacts_tree.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<StoredMessage>> {
        let messages_tree = self.db.open_tree("messages")?;
        let mut messages = Vec::new();

        for item in messages_tree.iter().rev() {
            let (_, value) = item?;
            let message: StoredMessage = bincode::deserialize(&value)?;

            if message.content.to_lowercase().contains(&query.to_lowercase()) {
                messages.push(message);

                if messages.len() >= limit {
                    break;
                }
            }
        }

        Ok(messages)
    }

    pub fn get_message_count(&self, chat_id: &str) -> Result<usize> {
        let messages_tree = self.db.open_tree("messages")?;
        let prefix = format!("_{}_", chat_id);
        let count = messages_tree.iter()
            .filter(|item| {
                if let Ok((key, _)) = item {
                    let key_str = String::from_utf8_lossy(key);
                    key_str.contains(&prefix)
                } else {
                    false
                }
            })
            .count();

        Ok(count)
    }

    pub fn update_last_seen(&self, peer_id: &PeerId, timestamp: i64) -> Result<()> {
        let contacts_tree = self.db.open_tree("contacts")?;

        let key = peer_id.to_string();

        let mut contact = if let Some(value) = contacts_tree.get(key.as_bytes())? {
            bincode::deserialize(&value)?
        } else {
            Contact {
                peer_id: peer_id.to_string(),
                name: None,
                last_seen: None,
            }
        };

        contact.last_seen = Some(timestamp);

        let value = bincode::serialize(&contact)?;
        contacts_tree.insert(key.as_bytes(), value)?;

        Ok(())
    }

    pub fn find_contact_by_name(&self, name: &str) -> Result<Option<Contact>> {
        let contacts_tree = self.db.open_tree("contacts")?;

        for item in contacts_tree.iter() {
            let (_, value) = item?;
            let contact: Contact = bincode::deserialize(&value)?;

            if let Some(contact_name) = &contact.name {
                if contact_name.eq_ignore_ascii_case(name) {
                    return Ok(Some(contact));
                }
            }
        }

        Ok(None)
    }

    pub fn clear(&self) -> Result<()> {
        self.db.clear()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_save_and_get_messages() {
        let temp_dir = env::temp_dir();
        let db_path = temp_dir.join("test_messages.db");

        let storage = Storage::new(&db_path).unwrap();

        let message = StoredMessage {
            id: "msg1".to_string(),
            chat_id: "chat1".to_string(),
            sender: "peer1".to_string(),
            content: "Hello!".to_string(),
            timestamp: 1234567890,
            is_outgoing: false,
        };

        storage.save_message(&message).unwrap();

        let messages = storage.get_messages("chat1", 10).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");

        storage.clear().unwrap();
        std::fs::remove_dir_all(db_path).ok();
    }

    #[test]
    fn test_save_and_get_contacts() {
        let temp_dir = env::temp_dir();
        let db_path = temp_dir.join("test_contacts.db");

        let storage = Storage::new(&db_path).unwrap();

        let contact = Contact {
            peer_id: "peer123".to_string(),
            name: Some("Alice".to_string()),
            last_seen: Some(1234567890),
        };

        storage.save_contact(&contact).unwrap();

        let contacts = storage.get_contacts().unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].name, Some("Alice".to_string()));

        storage.clear().unwrap();
        std::fs::remove_dir_all(db_path).ok();
    }
}