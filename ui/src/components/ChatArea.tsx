import { useEffect, useState, useRef } from 'react';
import { api } from '../api';
import type { Contact, Message } from '../types';
import { useNetworkEvents } from '../hooks/useNetworkEvents';

interface ChatAreaProps {
  contact: Contact;
}

export function ChatArea({ contact }: ChatAreaProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [loading, setLoading] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const loadMessages = async () => {
    try {
      const data = await api.getMessages(contact.peer_id);
      setMessages(data);
    } catch (error) {
      console.error('Failed to load messages:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadMessages();
  }, [contact.peer_id]);

  useNetworkEvents({
    onMessageReceived: (event) => {
      console.log('Message received event:', event);
      if (event.peer_id === contact.peer_id) {
        console.log('Reloading messages for', contact.peer_id);
        loadMessages();
      }
    },
  });

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim()) return;

    try {
      await api.sendMessage(contact.peer_id, inputValue);
      setInputValue('');
      loadMessages(); // Reload to show sent message
    } catch (error) {
      console.error('Failed to send message:', error);
      alert('Failed to send message: ' + error);
    }
  };

  if (loading) {
    return (
      <div className="chat-area">
        <div className="chat-header">
          <h2>{contact.name || 'Unknown'}</h2>
          <p className="peer-id">{contact.peer_id}</p>
        </div>
        <div className="messages-container">
          <p>Loading messages...</p>
        </div>
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
            <div
              key={msg.id}
              className={`message ${msg.is_outgoing ? 'outgoing' : 'incoming'}`}
            >
              <div className="message-content">{msg.content}</div>
              <div className="message-time">
                {new Date(msg.timestamp * 1000).toLocaleTimeString()}
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
        <button type="submit" className="send-button">
          Send
        </button>
      </form>
    </div>
  );
}