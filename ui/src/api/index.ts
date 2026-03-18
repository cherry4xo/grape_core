import { invoke } from '@tauri-apps/api/core';
import type { Contact, Message, FileTransfer } from '../types';

export const api = {
  // Peer ID
  async getPeerId(): Promise<string> {
    return invoke('get_peer_id');
  },

  // Contacts
  async getContacts(): Promise<Contact[]> {
    return invoke('get_contacts');
  },

  async getChatPeers(): Promise<Contact[]> {
    return invoke('get_chat_peers');
  },

  async addContact(peerId: string, name?: string): Promise<void> {
    return invoke('add_contact', { peerId, name });
  },

  async removeContact(peerId: string): Promise<void> {
    return invoke('remove_contact', { peerId });
  },

  async renameContact(peerId: string, name: string | null): Promise<void> {
    return invoke('rename_contact', { peerId, name });
  },

  // Messages
  async getMessages(chatId: string, limit?: number): Promise<Message[]> {
    return invoke('get_messages', { chatId, limit });
  },

  async sendMessage(peerId: string, message: string): Promise<void> {
    return invoke('send_message', { peerId, message });
  },

  async searchMessages(query: string, limit?: number): Promise<Message[]> {
    return invoke('search_messages', { query, limit });
  },

  // Connection
  async dialPeer(address: string): Promise<void> {
    return invoke('dial_peer', { address });
  },

  // Channels
  async subscribeChannel(topic: string): Promise<void> {
    return invoke('subscribe_channel', { topic });
  },

  async unsubscribeChannel(topic: string): Promise<void> {
    return invoke('unsubscribe_channel', { topic });
  },

  async publishToChannel(topic: string, message: string): Promise<void> {
    return invoke('publish_to_channel', { topic, message });
  },

  async listChannels(): Promise<void> {
    return invoke('list_channels');
  },

  // DHT
  async getDhtStats(): Promise<void> {
    return invoke('get_dht_stats');
  },

  async triggerBootstrap(): Promise<void> {
    return invoke('trigger_bootstrap');
  },

  async findPeer(peerId: string): Promise<void> {
    return invoke('find_peer', { peerId });
  },

  async sendFile(peerId: string, filePath: string, caption: string = ''): Promise<void> {
    return invoke('send_file', { peerId, filePath, caption });
  },

  async getFileTransfers(chatId: string): Promise<FileTransfer[]> {
    return invoke('get_file_transfers', { chatId });
  },

  async openFile(filePath: string): Promise<void> {
    return invoke('open_file', { filePath });
  },

  async sendCallSignal(peerId: string, callId: string, signalJson: string): Promise<void> {
    return invoke('send_call_signal', { peerId, callId, signalJson });
  }
};
