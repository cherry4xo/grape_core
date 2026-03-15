import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useNetworkEvents } from './hooks/useNetworkEvents';
import { ContactList } from './components/ContactList';
import { ChatArea } from './components/ChatArea';
import { AuthScreen } from './components/AuthScreen';
import { api } from './api';
import type { Contact } from './types';
import './styles/index.css';

function App() {
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [authChecked, setAuthChecked] = useState(false);
  const [peerId, setPeerId] = useState<string>('');
  const [selectedContact, setSelectedContact] = useState<Contact | null>(null);
  const [showAddContactModal, setShowAddContactModal] = useState(false);
  const [newContactPeerId, setNewContactPeerId] = useState('');
  const [newContactName, setNewContactName] = useState('');
  const [contactsRefreshTrigger, setContactsRefreshTrigger] = useState(0);
  const [needsPassword, setNeedsPassword] = useState(false);

  useEffect(() => {
    checkAuth();
  }, []);

  async function checkAuth() {
    try {
      const hasSeed = await invoke<boolean>('auth_has_seed');
      if (!hasSeed) {
        setAuthChecked(true);
        return;
      }
      const isEncrypted = await invoke<boolean>('auth_is_encrypted');
      if (isEncrypted) {
        setNeedsPassword(true);
        setAuthChecked(true);
      } else {
        await invoke('auth_load_seed', { password: null });
        setIsAuthenticated(true);
        setAuthChecked(true);
      }
    } catch (e) {
      console.error('Auth check failed:', e);
      setAuthChecked(true);
    }
  }

  useEffect(() => {
    if (!isAuthenticated) return;
    loadPeerId();
    const unlistenPromise = listen('listen-addr-added', () => {
      loadPeerId();
    });
    return () => { unlistenPromise.then((fn: () => void) => fn()); };
  }, [isAuthenticated]);

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
    for (let i = 0; i < 20; i++) {
      try {
        const id = await api.getPeerId();
        setPeerId(id);
        return;
      } catch {
        await new Promise(r => setTimeout(r, 500));
      }
    }
    console.error('Failed to get peer ID after retries');
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

  if (!authChecked) {
    return (
      <div className="auth-screen">
        <div className="auth-card"><p>Loading...</p></div>
      </div>
    );
  }

  if (!isAuthenticated) {
    return (
      <AuthScreen
        onAuthenticated={() => setIsAuthenticated(true)}
        initialStep={needsPassword ? 'unlock' : 'welcome'}
      />
    );
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
