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
    pub delivery_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub peer_id: String,
    pub name: Option<String>,
    pub last_seen: Option<i64>,
    pub is_manual: bool,
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

    pub fn db_tree(&self, name: &str) -> Result<sled::Tree> {
        Ok(self.db.open_tree(name)?)
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

    pub fn mark_peer_messages_delivered(&self, peer_id: &str) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        for item in tree.iter() {
            let (key, value) = item?;
            let mut msg: StoredMessage = bincode::deserialize(&value)?;
            if msg.is_outgoing
                && msg.chat_id == peer_id
                && msg.delivery_status == "sent"
            {
                msg.delivery_status = "delivered".to_string();
                tree.insert(key, bincode::serialize(&msg)?)?;
            }
        }
        Ok(())
    }

    pub fn update_message_status(&self, message_id: &str, status: &str) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        for item in tree.iter() {
            let (key, value) = item?;
            let mut msg: StoredMessage = bincode::deserialize(&value)?;
            if msg.id == message_id {
                msg.delivery_status = status.to_string();
                tree.insert(key, bincode::serialize(&msg)?)?;
                return Ok(());
            }
        }
        Ok(())
    }

    pub fn save_pending_message(&self, peer_id: &str, content: &str, message_id: &str) -> Result<()> {
        let tree = self.db.open_tree("pending_messages")?;
        let key = format!("{}_{}", peer_id, message_id);
        let value = serde_json::json!({
            "peer_id": peer_id,
            "content": content,
            "message_id": message_id
        }).to_string();
        tree.insert(key.as_bytes(), value.as_bytes())?;
        Ok(())
    }

    pub fn get_pending_messages(&self, peer_id: &str) -> Result<Vec<(String, String)>> {
        let tree = self.db.open_tree("pending_messages")?;
        let prefix = format!("{}_", peer_id);
        let mut result = Vec::new();
        for item in tree.iter() {
            let (key, value) = item?;
            if String::from_utf8_lossy(&key).starts_with(&prefix) {
                let json: serde_json::Value = serde_json::from_slice(&value)?;
                let msg_id = json["message_id"].as_str().unwrap_or("").to_string();
                let content = json["content"].as_str().unwrap_or("").to_string();
                result.push((msg_id, content));
            }
        }

        Ok(result)
    }

    pub fn delete_pending_message(&self, peer_id: &str, message_id: &str) -> Result<()> {
        let tree = self.db.open_tree("pending_messages")?;
        tree.remove(format!("{}_{}", peer_id, message_id))?;
        Ok(())
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
        if let Some(value) = contacts_tree.get(key.as_bytes())? {
            let mut contact: Contact = bincode::deserialize(&value)?;
            contact.last_seen = Some(timestamp);
            contacts_tree.insert(key.as_bytes(), bincode::serialize(&contact)?)?;
        }
        Ok(())
    }

    pub fn save_seen_peer(&self, peer_id: &PeerId, timestamp: i64) -> Result<()> {
        let contacts_tree = self.db.open_tree("contacts")?;
        let key = peer_id.to_string();
        if contacts_tree.get(key.as_bytes())?.is_none() {
            let contact = Contact {
                peer_id: key.clone(),
                name: None,
                last_seen: Some(timestamp),
                is_manual: false,
            };
            contacts_tree.insert(key.as_bytes(), bincode::serialize(&contact)?)?;
        } else {
            self.update_last_seen(peer_id, timestamp)?;
        }
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
            delivery_status: "sent".to_string(),
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
            is_manual: true,
        };

        storage.save_contact(&contact).unwrap();

        let contacts = storage.get_contacts().unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].name, Some("Alice".to_string()));

        storage.clear().unwrap();
        std::fs::remove_dir_all(db_path).ok();
    }
}