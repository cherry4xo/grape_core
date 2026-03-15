use crate::auth::AuthManager;
use crate::network::{NetworkCommand, NetworkEvent};
use crate::storage::{Storage, Contact};
use libp2p::{Multiaddr, PeerId};
pub use libp2p::identity::Keypair as LibP2pKeypair;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::{broadcast, mpsc, Mutex, oneshot};

pub struct PendingAuth {
    pub unlock_tx: Arc<Mutex<Option<oneshot::Sender<libp2p::identity::Keypair>>>>,
}

// App state that will be shared across Tauri commands
pub struct AppState {
    pub command_tx: mpsc::Sender<NetworkCommand>,
    pub event_rx: Arc<Mutex<broadcast::Receiver<NetworkEvent>>>,
    pub storage: Arc<Storage>,
    pub peer_id: String,
}

// DTOs for frontend communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactDto {
    pub peer_id: String,
    pub name: Option<String>,
    pub last_seen: Option<i64>,
    pub is_manual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub chat_id: String,
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
    pub is_outgoing: bool,
    pub delivery_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub addresses: Vec<String>,
}

// Tauri commands

#[tauri::command]
pub async fn get_peer_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.peer_id.clone())
}

#[tauri::command]
pub async fn get_contacts(state: State<'_, AppState>) -> Result<Vec<ContactDto>, String> {
    let contacts = state.storage.get_contacts()
        .map_err(|e| format!("Failed to get contacts: {:?}", e))?;

    Ok(contacts.into_iter().map(|c| ContactDto {
        peer_id: c.peer_id,
        name: c.name,
        last_seen: c.last_seen,
        is_manual: c.is_manual,
    }).collect())
}

#[tauri::command]
pub async fn add_contact(
    name: Option<String>,
    peer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let contact = Contact {
        peer_id,
        name,
        last_seen: Some(chrono::Utc::now().timestamp()),
        is_manual: true,
    };

    state.storage.save_contact(&contact)
        .map_err(|e| format!("Failed to save contact: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn rename_contact(
    peer_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let contacts_tree = state.storage.db_tree("contacts")
        .map_err(|e| format!("Failed to open contacts: {:?}", e))?;

    if let Some(value) = contacts_tree.get(peer_id.as_bytes())
        .map_err(|e| format!("DB error: {:?}", e))?
    {
        let mut contact: crate::storage::Contact = bincode::deserialize(&value)
            .map_err(|e| format!("Deserialize error: {:?}", e))?;
        contact.name = name;
        contacts_tree.insert(peer_id.as_bytes(), bincode::serialize(&contact)
            .map_err(|e| format!("Serialize error: {:?}", e))?)
            .map_err(|e| format!("DB insert error: {:?}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn remove_contact(
    peer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let peer_id = peer_id.parse::<PeerId>()
        .map_err(|e| format!("Invalid peer_id: {:?}", e))?;

    state.storage.remove_contact(&peer_id)
        .map_err(|e| format!("Failed to remove contact: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn get_messages(
    chat_id: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<MessageDto>, String> {
    let limit = limit.unwrap_or(50) as usize;

    let messages = state.storage.get_messages(&chat_id, limit)
        .map_err(|e| format!("Failed to get messages: {:?}", e))?;

    Ok(messages.into_iter().map(|m| MessageDto {
        id: m.id,
        chat_id: m.chat_id,
        sender: m.sender,
        content: m.content,
        timestamp: m.timestamp,
        is_outgoing: m.is_outgoing,
        delivery_status: m.delivery_status,
    }).collect())
}

#[tauri::command]
pub async fn send_message(
    peer_id: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let peer_id = peer_id.parse::<PeerId>()
        .map_err(|e| format!("Invalid peer_id: {:?}", e))?;

    state.command_tx.send(NetworkCommand::SendMessage {
        peer_id,
        message,
    }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn dial_peer(
    address: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let address: Multiaddr = address.parse()
        .map_err(|e| format!("Invalid multiaddr: {:?}", e))?;

    state.command_tx.send(NetworkCommand::Dial { address }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn search_messages(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<MessageDto>, String> {
    let limit = limit.unwrap_or(50) as usize;

    let messages = state.storage.search_messages(&query, limit)
        .map_err(|e| format!("Failed to search messages: {:?}", e))?;

    Ok(messages.into_iter().map(|m| MessageDto {
        id: m.id,
        chat_id: m.chat_id,
        sender: m.sender,
        content: m.content,
        timestamp: m.timestamp,
        is_outgoing: m.is_outgoing,
        delivery_status: m.delivery_status,
    }).collect())
}

#[tauri::command]
pub async fn subscribe_channel(
    topic: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::SubscribeChannel { topic }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn unsubscribe_channel(
    topic: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::UnsubscribeChannel { topic }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn publish_to_channel(
    topic: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::PublishToChannel { topic, message }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn list_channels(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::ListChannels).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

fn seed_path() -> std::path::PathBuf {
    let base = std::env::var("VIDEOCALLS_STORAGE_PATH")
        .unwrap_or_else(|_| ".videocalls".to_string());
    std::path::PathBuf::from(base).join("seed.enc")
}

#[tauri::command]
pub async fn auth_has_seed() -> Result<bool, String> {
    Ok(seed_path().exists())
}

#[tauri::command]
pub async fn auth_is_encrypted() -> Result<bool, String> {
    let path = seed_path();
    if !path.exists() { return Ok(false); }
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::from_slice::<crate::auth::EncryptedSeed>(&data).is_ok())
}

#[tauri::command]
pub async fn auth_generate_mnemonic() -> Result<String, String> {
    let mut auth = AuthManager::new();
    auth.generate_mnemonic()
        .map_err(|e| format!("Failed to generate mnemonic: {}", e))
}

#[tauri::command]
pub async fn auth_validate_mnemonic(words: String) -> Result<bool, String> {
    Ok(AuthManager::validate_mnemonic(&words))
}

#[tauri::command]
pub async fn auth_save_seed(words: String, password: Option<String>) -> Result<(), String> {
    let mut auth = AuthManager::new();
    auth.import_mnemonic(&words)
        .map_err(|e| format!("Invalid mnemonic: {}", e))?;
    auth.save(seed_path(), password.as_deref())
        .map_err(|e| format!("Failed to save seed: {}", e))
}

#[tauri::command]
pub async fn auth_load_seed(password: Option<String>) -> Result<String, String> {
    let auth = AuthManager::load(seed_path(), password.as_deref())
        .map_err(|e| format!("Failed to load seed: {}", e))?;
    let keypair = auth.derive_keypair()
        .map_err(|e| format!("Failed to derive keypair: {}", e))?;
    Ok(keypair.public().to_peer_id().to_string())
}

#[tauri::command]
pub async fn get_dht_stats(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::GetDhtStats).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn trigger_bootstrap(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.command_tx.send(NetworkCommand::Bootstrap).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn find_peer(
    peer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let peer_id = peer_id.parse::<PeerId>()
        .map_err(|e| format!("Invalid peer_id: {:?}", e))?;

    state.command_tx.send(NetworkCommand::FindPeer { peer_id }).await
        .map_err(|e| format!("Failed to send command: {:?}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn auth_unlock(
    password: String,
    state: State<'_, PendingAuth>,
) -> Result<String, String> {
    let auth = AuthManager::load(seed_path(), Some(&password))
        .map_err(|e| format!("Wrong password: {}", e))?;
    let keypair = auth.derive_keypair()
        .map_err(|e| format!("Key derivation failed: {}", e))?;
    let peer_id = keypair.public().to_peer_id().to_string();
    let mut guard = state.unlock_tx.lock().await;
    if let Some(tx) = guard.take() {
        let _ = tx.send(keypair);
    }

    Ok(peer_id)
}