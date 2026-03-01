import { useEffect, useState } from 'react';
import { useNetworkEvents } from './hooks/useNetworkEvents';
import { ContactList } from './components/ContactList';
import { ChatArea } from './components/ChatArea';
import { api } from './api';
import type { Contact } from './types';
import './styles/index.css';

function App() {
  const [peerId, setPeerId] = useState<string>('');
  const [selectedContact, setSelectedContact] = useState<Contact | null>(null);
  const [showAddContactModal, setShowAddContactModal] = useState(false);
  const [newContactPeerId, setNewContactPeerId] = useState('');
  const [newContactName, setNewContactName] = useState('');
  const [contactsRefreshTrigger, setContactsRefreshTrigger] = useState(0);

  useEffect(() => {
    loadPeerId();
  }, []);

  // Real-time event listeners
  useNetworkEvents({
    onPeerConnected: () => {
      console.log('Peer connected, refreshing contacts');
      setContactsRefreshTrigger(prev => prev + 1);
    },
    onPeerDisconnected: () => {
      console.log('Peer disconnected, refreshing contacts');
      setContactsRefreshTrigger(prev => prev + 1);
    },
  });

  async function loadPeerId() {
    try {
      const id = await api.getPeerId();
      setPeerId(id);
    } catch (error) {
      console.error('Failed to get peer ID:', error);
    }
  }

  async function handleAddContact(e: React.FormEvent) {
    e.preventDefault();

    if (!newContactPeerId.trim()) {
      alert('Peer ID is required');
      return;
    }

    try {
      await api.addContact(newContactPeerId, newContactName || undefined);
      setShowAddContactModal(false);
      setNewContactPeerId('');
      setNewContactName('');

      // Trigger reload of contact list
      setContactsRefreshTrigger(prev => prev + 1);
    } catch (error) {
      console.error('Failed to add contact:', error);
      alert('Failed to add contact: ' + error);
    }
  }

  return (
    <div className="container">
      <div className="sidebar">
        <div className="header">
          <h1>VideoCalls</h1>
          <div className="peer-id">
            Your ID: {peerId.slice(0, 8)}...{peerId.slice(-8)}
          </div>
          <button
            onClick={() => setShowAddContactModal(true)}
            style={{ marginTop: '12px', width: '100%' }}
          >
            Add Contact
          </button>
        </div>

        <ContactList
          onSelectContact={setSelectedContact}
          selectedContact={selectedContact}
          refreshTrigger={contactsRefreshTrigger}
        />
      </div>

      <div className="main-content">
        {selectedContact ? (
          <ChatArea contact={selectedContact} />
        ) : (
          <div className="empty-state">
            Select a contact to start chatting
          </div>
        )}
      </div>

      {showAddContactModal && (
        <div className="modal-overlay" onClick={() => setShowAddContactModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Add Contact</div>

            <form onSubmit={handleAddContact}>
              <div className="form-group">
                <label className="form-label">Peer ID *</label>
                <input
                  type="text"
                  className="form-input"
                  placeholder="12D3KooW..."
                  value={newContactPeerId}
                  onChange={(e) => setNewContactPeerId(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label className="form-label">Name (optional)</label>
                <input
                  type="text"
                  className="form-input"
                  placeholder="Alice"
                  value={newContactName}
                  onChange={(e) => setNewContactName(e.target.value)}
                />
              </div>

              <div className="modal-buttons">
                <button
                  type="button"
                  className="button-secondary"
                  onClick={() => setShowAddContactModal(false)}
                >
                  Cancel
                </button>
                <button type="submit">Add</button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
