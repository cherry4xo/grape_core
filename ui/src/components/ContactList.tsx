import { useEffect, useState } from 'react';
import { api } from '../api';
import type { Contact } from '../types';

interface ContactListProps {
  onSelectContact: (contact: Contact) => void;
  selectedContact: Contact | null;
  refreshTrigger?: number;
}

export function ContactList({ onSelectContact, selectedContact, refreshTrigger }: ContactListProps) {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadContacts();
  }, [refreshTrigger]);

  async function loadContacts() {
    try {
      const data = await api.getContacts();
      setContacts(data);
    } catch (error) {
      console.error('Failed to load contacts:', error);
    } finally {
      setLoading(false);
    }
  }

  function formatPeerId(peerId: string): string {
    if (peerId.length > 20) {
      return `${peerId.slice(0, 8)}...${peerId.slice(-8)}`;
    }
    return peerId;
  }

  if (loading) {
    return <div style={{ padding: '16px' }}>Loading contacts...</div>;
  }

  return (
    <div className="contact-list">
      {contacts.length === 0 ? (
        <div style={{ padding: '16px', color: '#666' }}>
          No contacts yet. Add a contact to start chatting.
        </div>
      ) : (
        contacts.map((contact) => (
          <div
            key={contact.peer_id}
            className={`contact-item ${selectedContact?.peer_id === contact.peer_id ? 'active' : ''}`}
            onClick={() => onSelectContact(contact)}
          >
            <div className="contact-name">
              {contact.name || 'Unknown'}
            </div>
            <div className="contact-peer-id">
              {formatPeerId(contact.peer_id)}
            </div>
          </div>
        ))
      )}
    </div>
  );
}
