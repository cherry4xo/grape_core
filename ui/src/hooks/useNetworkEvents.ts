import { useEffect, useRef } from 'react';
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
  const callbacksRef = useRef(callbacks);
  useEffect(() => {
    callbacksRef.current = callbacks;
  });

  useEffect(() => {
    const unlistenPromises: Promise<UnlistenFn>[] = [
      listen<PeerConnectedEvent>('peer-connected', (event) => {
        callbacksRef.current.onPeerConnected?.(event.payload);
      }),
      listen<PeerConnectedEvent>('peer-disconnected', (event) => {
        callbacksRef.current.onPeerDisconnected?.(event.payload);
      }),
      listen<MessageReceivedEvent>('message-received', (event) => {
        callbacksRef.current.onMessageReceived?.(event.payload);
      }),
      listen<ChannelMessageEvent>('channel-message', (event) => {
        callbacksRef.current.onChannelMessage?.(event.payload);
      }),
    ];

    return () => {
      Promise.all(unlistenPromises).then((unlisteners) => {
        unlisteners.forEach((u) => u());
      });
    };
  }, []); // subscribe once on mount
}