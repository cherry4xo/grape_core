export interface Contact {
  peer_id: string;
  name: string | null;
  last_seen: number | null;
}

export interface Message {
  id: string;
  chat_id: string;
  sender: string;
  content: string;
  timestamp: number;
  is_outgoing: boolean;
}

export interface PeerInfo {
  peer_id: string;
  addresses: string[];
}
