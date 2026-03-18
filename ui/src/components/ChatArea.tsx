import { useEffect, useState, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../api';
import type { Contact, Message, FileTransfer, CallType } from '../types';

interface ChatAreaProps {
  contact: Contact;
  onStartCall: (callType: CallType) => void;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function isImage(mimeType: string | null, fileName: string): boolean {
  if (mimeType?.startsWith('image/')) return true;
  return /\.(jpg|jpeg|png|gif|webp|svg|bmp)$/i.test(fileName);
}

export function ChatArea({ contact, onStartCall }: ChatAreaProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [fileTransfers, setFileTransfers] = useState<FileTransfer[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [loading, setLoading] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const contactRef = useRef(contact);
  const isInitialLoad = useRef(true);

  useEffect(() => { contactRef.current = contact; });

  const loadAll = useCallback(async () => {
    try {
      const [msgs, transfers] = await Promise.all([
        api.getMessages(contact.peer_id),
        api.getFileTransfers(contact.peer_id),
      ]);
      setMessages(msgs);
      setFileTransfers(transfers);
    } catch (e) {
      console.error('Failed to load chat:', e);
    } finally {
      setLoading(false);
    }
  }, [contact.peer_id]);

  useEffect(() => {
    setLoading(true);
    setMessages([]);
    setFileTransfers([]);
    isInitialLoad.current = true;
    loadAll();
  }, [contact.peer_id]);

  useEffect(() => {
    const unlistenMsg = listen<{ peer_id: string; message: string }>(
      'message-received',
      (event) => {
        if (event.payload.peer_id !== contactRef.current.peer_id) return;
        setMessages(prev => [...prev, {
          id: crypto.randomUUID(),
          chat_id: event.payload.peer_id,
          sender: event.payload.peer_id,
          content: event.payload.message,
          timestamp: Math.floor(Date.now() / 1000),
          is_outgoing: false,
          delivery_status: 'delivered',
        }]);
      }
    );

    const unlistenDelivered = listen<{ peer_id: string; message_id: string }>(
      'message-delivered',
      (event) => {
        if (event.payload.peer_id !== contactRef.current.peer_id) return;
        setMessages(prev =>
          prev.map(m =>
            m.id === event.payload.message_id
              ? { ...m, delivery_status: 'delivered' as const }
              : m
          )
        );
      }
    );

    const unlistenFileReceived = listen<{
      peer_id: string;
      transfer_id: string;
      message_id: string;
      file_name: string;
      file_path: string;
      size: number;
      mime_type: string | null;
      caption: string;
    }>('file-received', (event) => {
      if (event.payload.peer_id !== contactRef.current.peer_id) return;
      setFileTransfers(prev => {
        const exists = prev.find(f => f.id === event.payload.transfer_id);
        if (exists) {
          return prev.map(f =>
            f.id === event.payload.transfer_id
              ? { ...f, status: 'completed' as const, local_path: event.payload.file_path }
              : f
          );
        }
        return [...prev, {
          id: event.payload.transfer_id,
          chat_id: event.payload.peer_id,
          file_name: event.payload.file_name,
          file_size: event.payload.size,
          total_chunks: 1,
          chunks_done: 1,
          local_path: event.payload.file_path,
          is_outgoing: false,
          status: 'completed' as const,
          timestamp: Math.floor(Date.now() / 1000),
          mime_type: event.payload.mime_type,
        }];
      });
      api.getMessages(contactRef.current.peer_id).then(setMessages).catch(console.error);
    });

    const unlistenProgress = listen<{
      peer_id: string;
      transfer_id: string;
      chunks_done: number;
      total_chunks: number;
    }>('file-transfer-progress', (event) => {
      if (event.payload.peer_id !== contactRef.current.peer_id) return;
      setFileTransfers(prev => prev.map(ft =>
        ft.id === event.payload.transfer_id
          ? {
              ...ft,
              chunks_done: event.payload.chunks_done,
              total_chunks: event.payload.total_chunks,
              status: 'in_progress' as const,
            }
          : ft
      ));
    });

    const unlistenFailed = listen<{ peer_id: string; transfer_id: string }>(
      'file-transfer-failed',
      (event) => {
        if (event.payload.peer_id !== contactRef.current.peer_id) return;
        setFileTransfers(prev => prev.map(ft =>
          ft.id === event.payload.transfer_id
            ? { ...ft, status: 'failed' as const }
            : ft
        ));
      }
    );

    const unlistenCompleted = listen<{ peer_id: string; transfer_id: string }>(
      'file-transfer-completed',
      (event) => {
        if (event.payload.peer_id !== contactRef.current.peer_id) return;
        setFileTransfers(prev => prev.map(ft =>
          ft.id === event.payload.transfer_id
            ? { ...ft, status: 'completed' as const, chunks_done: ft.total_chunks }
            : ft
        ));
      }
    );

    return () => {
      unlistenMsg.then(fn => fn());
      unlistenDelivered.then(fn => fn());
      unlistenFileReceived.then(fn => fn());
      unlistenProgress.then(fn => fn());
      unlistenFailed.then(fn => fn());
      unlistenCompleted.then(fn => fn());
    };
  }, []);

  useEffect(() => {
    if (isInitialLoad.current) {
      isInitialLoad.current = false;
      // On initial load, scroll instantly without animation
      messagesEndRef.current?.scrollIntoView({ behavior: 'instant' });
      return;
    }
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, fileTransfers]);

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim()) return;

    const tempId = crypto.randomUUID();
    const content = inputValue;

    setMessages(prev => [...prev, {
      id: tempId,
      chat_id: contact.peer_id,
      sender: 'me',
      content,
      timestamp: Math.floor(Date.now() / 1000),
      is_outgoing: true,
      delivery_status: 'sent',
    }]);
    setInputValue('');

    try {
      await api.sendMessage(contact.peer_id, content);
      const data = await api.getMessages(contact.peer_id);
      setMessages(data);
    } catch (error) {
      console.error('Failed to send message:', error);
      setMessages(prev => prev.filter(m => m.id !== tempId));
      setInputValue(content);
      alert('Failed to send message: ' + error);
    }
  };

  const handleFilePick = async () => {
    const selected = await openDialog({ multiple: false, directory: false });
    if (!selected) return;
    const filePath = selected as string;
    const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'file';

    const tempTransferId = crypto.randomUUID();
    const tempMsgId = crypto.randomUUID();

    setFileTransfers(prev => [...prev, {
      id: tempTransferId,
      chat_id: contact.peer_id,
      file_name: fileName,
      file_size: 0,
      total_chunks: 0,
      chunks_done: 0,
      local_path: null,
      is_outgoing: true,
      status: 'pending',
      timestamp: Math.floor(Date.now() / 1000),
      mime_type: null,
    }]);
    setMessages(prev => [...prev, {
      id: tempMsgId,
      chat_id: contact.peer_id,
      sender: 'me',
      content: '',
      timestamp: Math.floor(Date.now() / 1000),
      is_outgoing: true,
      delivery_status: 'sent',
      file_transfer_id: tempTransferId,
    }]);

    try {
      await api.sendFile(contact.peer_id, filePath, '');
      const [msgs, transfers] = await Promise.all([
        api.getMessages(contact.peer_id),
        api.getFileTransfers(contact.peer_id),
      ]);
      setMessages(msgs);
      setFileTransfers(transfers);
    } catch (err) {
      setFileTransfers(prev => prev.filter(ft => ft.id !== tempTransferId));
      setMessages(prev => prev.filter(m => m.id !== tempMsgId));
      alert('Failed to send file: ' + err);
    }
  };

  const statusIcon = (msgStatus: string, ft?: FileTransfer) => {
    if (ft) {
      if (ft.status === 'queued' || ft.status === 'pending') return ' ⏳';
      if (ft.status === 'in_progress') return ' ✓';
      if (ft.status === 'completed') return ' ✓✓';
      if (ft.status === 'failed') return ' ✗';
      return '';
    }
    if (msgStatus === 'queued') return ' ⏳';
    if (msgStatus === 'sent') return ' ✓';
    if (msgStatus === 'delivered') return ' ✓✓';
    return '';
  };

  const ftMap = new Map(fileTransfers.map(ft => [ft.id, ft]));

  if (loading) {
    return (
      <div className="chat-area">
        <div className="chat-header">
          <div className="chat-header-info">
            <h2>{contact.name || 'Unknown'}</h2>
            <p className="peer-id">{contact.peer_id}</p>
          </div>
          <div className="chat-header-actions">
            <button className="call-btn" onClick={() => onStartCall('audio')} title="Audio call">📞</button>
            <button className="call-btn" onClick={() => onStartCall('video')} title="Video call">📹</button>
          </div>
        </div>
        <div className="messages-container"><p>Loading messages...</p></div>
      </div>
    );
  }

  return (
    <div className="chat-area">
      <div className="chat-header">
        <div className="chat-header-info">
          <h2>{contact.name || 'Unknown'}</h2>
          <p className="peer-id">{contact.peer_id}</p>
        </div>
        <div className="chat-header-actions">
          <button className="call-btn" onClick={() => onStartCall('audio')} title="Audio call">📞</button>
          <button className="call-btn" onClick={() => onStartCall('video')} title="Video call">📹</button>
        </div>
      </div>

      <div className="messages-container">
        {messages.length === 0 ? (
          <p className="no-messages">No messages yet. Send a message to start the conversation!</p>
        ) : (
          messages.map((msg) => {
            const ft = msg.file_transfer_id ? ftMap.get(msg.file_transfer_id) : undefined;
            return (
              <div key={msg.id} className={`message ${msg.is_outgoing ? 'outgoing' : 'incoming'}`}>
                {ft ? (
                  <FileAttachment ft={ft} caption={msg.content} />
                ) : (
                  <div className="message-content">{msg.content}</div>
                )}
                <div className="message-time">
                  {new Date(msg.timestamp * 1000).toLocaleTimeString()}
                  {msg.is_outgoing && (
                    <span className="delivery-status">{statusIcon(msg.delivery_status, ft)}</span>
                  )}
                </div>
              </div>
            );
          })
        )}
        <div ref={messagesEndRef} />
      </div>

      <form className="message-input-form" onSubmit={handleSendMessage}>
        <button type="button" className="attach-button" onClick={handleFilePick} title="Send file">
          📎
        </button>
        <input
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          placeholder="Type a message..."
          className="message-input"
        />
        <button type="submit" className="send-button">Send</button>
      </form>
    </div>
  );
}

function FileAttachment({ ft, caption }: { ft: FileTransfer; caption: string }) {
  const pct = ft.total_chunks > 0
    ? Math.round((ft.chunks_done / ft.total_chunks) * 100)
    : 0;
  const isImg = isImage(ft.mime_type, ft.file_name);

  return (
    <div className="file-attachment">
      <div className="file-doc">
        <span className="file-icon">{isImg ? '🖼' : '📎'}</span>
        <div className="file-doc-info">
          <span className="file-name">{ft.file_name}</span>
          <span className="file-size">{formatBytes(ft.file_size)}</span>
        </div>
        {ft.status === 'completed' && ft.local_path && (
          <button className="open-file-button" onClick={() => api.openFile(ft.local_path!)}>
            Open
          </button>
        )}
      </div>
      {ft.status === 'in_progress' && (
        <div className="progress-bar-container">
          <div className="progress-bar" style={{ width: `${pct}%` }} />
          <span className="progress-text">{pct}%</span>
        </div>
      )}
      {(ft.status === 'queued' || ft.status === 'pending') && (
        <span className="file-status">⏳ Queued</span>
      )}
      {ft.status === 'failed' && <span className="file-status error">✗ Failed</span>}
      {caption && <div className="file-caption">{caption}</div>}
    </div>
  );
}
