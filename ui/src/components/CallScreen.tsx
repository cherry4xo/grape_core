import { useEffect, useRef, useState } from 'react';
import type { CallChatMessage, CallState } from '../types';

function cslog(msg: string, data?: unknown) {
  const ts = new Date().toISOString().slice(11, 23);
  if (data !== undefined) console.log(`${ts} [CallScreen] ${msg}`, data);
  else console.log(`${ts} [CallScreen] ${msg}`);
}

interface CallScreenProps {
  callState: CallState;
  remoteVideoRef: React.RefObject<HTMLVideoElement>;
  localVideoRef: React.RefObject<HTMLVideoElement>;
  onHangup: () => void;
  onToggleMute: () => void;
  onToggleVideo: () => void;
  onToggleScreen: () => void;
  callMessages: CallChatMessage[];
  onSendCallMessage: (text: string) => void;
}

export function CallScreen({
  callState, remoteVideoRef, localVideoRef,
  onHangup, onToggleMute, onToggleVideo, onToggleScreen,
  callMessages, onSendCallMessage,
}: CallScreenProps) {
  const [elapsed, setElapsed] = useState(0);
  const [chatOpen, setChatOpen] = useState(false);
  const [draftText, setDraftText] = useState('');
  const [lastReadCount, setLastReadCount] = useState(0);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);

  const unreadCount = chatOpen ? 0 : callMessages.length - lastReadCount;

  const handleOpenChat = () => {
    setLastReadCount(callMessages.length);
    setChatOpen(true);
  };

  useEffect(() => {
    if (chatOpen) messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [callMessages, chatOpen]);

  useEffect(() => {
    if (!callState.startTime) return;
    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - callState.startTime!) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [callState.startTime]);

  const formatTime = (s: number) =>
    `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;

  const statusText = callState.status === 'calling' ? 'Calling...'
    : callState.status === 'ringing' ? 'Incoming call...'
    : callState.status === 'connected' ? formatTime(elapsed)
    : '';

  return (
    <div className="call-screen">
      <video ref={remoteVideoRef} className="remote-video" autoPlay playsInline />
      <video ref={localVideoRef} className="local-video" autoPlay playsInline muted />

      <div className="call-info">
        <div className="call-peer-name">{callState.peerName || callState.peerId?.slice(0, 8)}</div>
        <div className="call-status">{statusText}</div>
      </div>

      <div className="call-controls">
        <button
          className={`call-ctrl-btn ${callState.isMuted ? 'active' : ''}`}
          onClick={onToggleMute}
          title={callState.isMuted ? 'Unmute' : 'Mute'}
        >
          {callState.isMuted ? '🔇' : '🎙️'}
        </button>
        {callState.callType === 'video' && (
          <button
            className={`call-ctrl-btn ${callState.isVideoOff ? 'active' : ''}`}
            onClick={onToggleVideo}
            title={callState.isVideoOff ? 'Enable camera' : 'Disable camera'}
          >
            {callState.isVideoOff ? '📷' : '🎥'}
          </button>
        )}
        {callState.callType === 'video' && (
          <button
            className={`call-ctrl-btn ${callState.isSharingScreen ? 'active' : ''}`}
            onClick={onToggleScreen}
            title={callState.isSharingScreen ? 'Stop sharing' : 'Share screen'}
          >
            🖥️
          </button>
        )}
        <button
          className="call-ctrl-btn"
          style={{ position: 'relative' }}
          onClick={chatOpen ? () => setChatOpen(false) : handleOpenChat}
          title="Chat"
        >
          💬
          {unreadCount > 0 && <span className="call-chat-badge">{unreadCount}</span>}
        </button>
        <button className="call-ctrl-btn hangup-btn" onClick={onHangup} title="Hang up">
          📵
        </button>
      </div>

      {chatOpen && (
        <div className="call-chat-panel">
          <div className="call-chat-header">
            <span>In-call chat</span>
            <button onClick={() => setChatOpen(false)}>✕</button>
          </div>
          <div className="call-chat-messages">
            {callMessages.map(msg => (
              <div key={msg.id} className={`call-chat-msg ${msg.fromSelf ? 'self' : 'peer'}`}>
                <span className="call-chat-text">{msg.text}</span>
                <span className="call-chat-time">
                  {new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                </span>
              </div>
            ))}
            <div ref={messagesEndRef} />
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (draftText.trim()) {
                onSendCallMessage(draftText);
                setDraftText('');
              }
            }}
            className="call-chat-input-row"
          >
            <input
              value={draftText}
              onChange={e => setDraftText(e.target.value)}
              placeholder="Message..."
              className="call-chat-input"
            />
            <button type="submit">Send</button>
          </form>
        </div>
      )}
    </div>
  );
}
