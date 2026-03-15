import { useEffect, useState, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../api';
import type { Contact, Message } from '../types';

interface ChatAreaProps {
  contact: Contact;
}

export function ChatArea({ contact }: ChatAreaProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [loading, setLoading] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const contactRef = useRef(contact);

  useEffect(() => { contactRef.current = contact; });

  const loadMessages = useCallback(async () => {
    try {
      const data = await api.getMessages(contact.peer_id);
      setMessages(data);
    } catch (error) {
      console.error('Failed to load messages:', error);
    } finally {
      setLoading(false);
    }
  }, [contact.peer_id]);

  // Reload on contact switch
  useEffect(() => {
    setLoading(true);
    setMessages([]);
    loadMessages();
  }, [contact.peer_id]);

  // Incoming message — append directly, no reload
  useEffect(() => {
    const unlistenMsg = listen<{ peer_id: string; message: string }>(
      'message-received',
      (event) => {
        if (event.payload.peer_id !== contactRef.current.peer_id) return;
        const newMsg: Message = {
          id: crypto.randomUUID(),
          chat_id: event.payload.peer_id,
          sender: event.payload.peer_id,
          content: event.payload.message,
          timestamp: Math.floor(Date.now() / 1000),
          is_outgoing: false,
          delivery_status: 'delivered',
        };
        setMessages(prev => [...prev, newMsg]);
      }
    );

    // Delivery confirmation — update status in place
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

    return () => {
      unlistenMsg.then(fn => fn());
      unlistenDelivered.then(fn => fn());
    };
  }, []);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim()) return;

    const tempId = crypto.randomUUID();
    const content = inputValue;

    // Optimistic append
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
      // Replace optimistic with real data (gets real ID + queued status if offline)
      const data = await api.getMessages(contact.peer_id);
      setMessages(data);
    } catch (error) {
      console.error('Failed to send message:', error);
      setMessages(prev => prev.filter(m => m.id !== tempId));
      setInputValue(content);
      alert('Failed to send message: ' + error);
    }
  };

  const statusIcon = (status: string) => {
    if (status === 'queued') return ' ⏳';
    if (status === 'sent') return ' ✓';
    if (status === 'delivered') return ' ✓✓';
    return '';
  };

  if (loading) {
    return (
      <div className="chat-area">
        <div className="chat-header">
          <h2>{contact.name || 'Unknown'}</h2>
          <p className="peer-id">{contact.peer_id}</p>
        </div>
        <div className="messages-container"><p>Loading messages...</p></div>
      </div>
    );
  }

  return (
    <div className="chat-area">
      <div className="chat-header">
        <h2>{contact.name || 'Unknown'}</h2>
        <p className="peer-id">{contact.peer_id}</p>
      </div>

      <div className="messages-container">
        {messages.length === 0 ? (
          <p className="no-messages">No messages yet. Send a message to start the conversation!</p>
        ) : (
          messages.map((msg) => (
            <div key={msg.id} className={`message ${msg.is_outgoing ? 'outgoing' : 'incoming'}`}>
              <div className="message-content">{msg.content}</div>
              <div className="message-time">
                {new Date(msg.timestamp * 1000).toLocaleTimeString()}
                {msg.is_outgoing && (
                  <span className="delivery-status">{statusIcon(msg.delivery_status)}</span>
                )}
              </div>
            </div>
          ))
        )}
        <div ref={messagesEndRef} />
      </div>

      <form className="message-input-form" onSubmit={handleSendMessage}>
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