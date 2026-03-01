use crate::network::{NetworkCommand, NetworkEvent};
use crate::storage::{Storage, Contact};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::{broadcast, mpsc, Mutex};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub chat_id: String,
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
    pub is_outgoing: bool,
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
    };

    state.storage.save_contact(&contact)
        .map_err(|e| format!("Failed to save contact: {:?}", e))?;

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
