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
  file_transfer_id?: string;
}

export interface FileTransfer {
  id: string;
  chat_id: string;
  file_name: string;
  file_size: number;
  total_chunks: number;
  chunks_done: number;
  local_path: string | null;
  is_outgoing: boolean;
  status: 'pending' | 'queued' | 'in_progress' | 'completed' | 'failed';
  timestamp: number;
  mime_type: string | null;
}

export interface PeerInfo {
  peer_id: string;
  addresses: string[];
}

export type CallType = 'audio' | 'video';

export type BitrateLevel = 'high' | 'medium' | 'low';

export interface CallChatMessage {
  id: string;
  text: string;
  timestamp: number;
  fromSelf: boolean;
}

export type CallStatus = 'idle' | 'calling' | 'ringing' | 'connected' | 'ended';

export interface CallState {
  status: CallStatus;
  peerId: string | null;
  peerName: string | null;
  callId: string | null;
  callType: CallType;
  isMuted: boolean,
  isVideoOff: boolean;
  isSharingScreen: boolean;
  startTime: number | null;
  offerSdp?: string;
}