import { useEffect } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface PeerConnectedEvent {
  peer_id: string;
}

export interface MessageReceivedEvent {
  peer_id: string;
  message: string;
}

export interface ChannelMessageEvent {
  topic: string;
  peer_id: string;
  message: string;
}

export function useNetworkEvents(callbacks: {
  onPeerConnected?: (event: PeerConnectedEvent) => void;
  onPeerDisconnected?: (event: PeerConnectedEvent) => void;
  onMessageReceived?: (event: MessageReceivedEvent) => void;
  onChannelMessage?: (event: ChannelMessageEvent) => void;
}) {
  useEffect(() => {
    const unlistenPromises: Promise<UnlistenFn>[] = [];

    if (callbacks.onPeerConnected) {
      unlistenPromises.push(
        listen<PeerConnectedEvent>('peer-connected', (event) => {
          callbacks.onPeerConnected?.(event.payload);
        })
      );
    }

    if (callbacks.onPeerDisconnected) {
      unlistenPromises.push(
        listen<PeerConnectedEvent>('peer-disconnected', (event) => {
          callbacks.onPeerDisconnected?.(event.payload);
        })
      );
    }

    if (callbacks.onMessageReceived) {
      unlistenPromises.push(
        listen<MessageReceivedEvent>('message-received', (event) => {
          callbacks.onMessageReceived?.(event.payload);
        })
      );
    }

    if (callbacks.onChannelMessage) {
      unlistenPromises.push(
        listen<ChannelMessageEvent>('channel-message', (event) => {
          callbacks.onChannelMessage?.(event.payload);
        })
      );
    }

    return () => {
      Promise.all(unlistenPromises).then((unlisteners) => {
        unlisteners.forEach((unlisten) => unlisten());
      });
    };
  }, [callbacks]);
}