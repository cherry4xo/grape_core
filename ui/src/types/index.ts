export interface Contact {
  peer_id: string;
  name: string | null;
  last_seen: number | null;
  is_manual: boolean;
}

export interface Message {
  id: string;
  chat_id: string;
  sender: string;
  content: string;
  timestamp: number;
  is_outgoing: boolean;
  delivery_status: 'sent' | 'delivered' | 'queued';
}

export interface PeerInfo {
  peer_id: string;
  addresses: string[];
}
