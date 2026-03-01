import { invoke } from '@tauri-apps/api/core';
import type { Contact, Message } from '../types';

export const api = {
  // Peer ID
  async getPeerId(): Promise<string> {
    return invoke('get_peer_id');
  },

  // Contacts
  async getContacts(): Promise<Contact[]> {
    return invoke('get_contacts');
  },

  async addContact(peerId: string, name?: string): Promise<void> {
    return invoke('add_contact', { peerId, name });
  },

  async removeContact(peerId: string): Promise<void> {
    return invoke('remove_contact', { peerId });
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
};
