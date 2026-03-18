import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../api';
import type { Contact } from '../types';

interface ContactListProps {
  onSelectContact: (contact: Contact) => void;
  selectedContact: Contact | null;
  refreshTrigger?: number;
}

type Tab = 'contacts' | 'strangers';

export function ContactList({ onSelectContact, selectedContact, refreshTrigger }: ContactListProps) {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [chatPeers, setChatPeers] = useState<Contact[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<Tab>('contacts');
  const [searchQuery, setSearchQuery] = useState('');
  const [dhtSearchId, setDhtSearchId] = useState('');
  const [dhtSearching, setDhtSearching] = useState(false);
  const [editingPeerId, setEditingPeerId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState('');
  const [onlinePeers, setOnlinePeers] = useState<Set<string>>(new Set());
  const editInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    loadContacts();
  }, [refreshTrigger]);

  async function loadContacts() {
    try {
      const [all, chats] = await Promise.all([
        api.getContacts().catch(() => [] as Contact[]),
        api.getChatPeers().catch(() => [] as Contact[]),
      ]);
      setContacts(all);
      setChatPeers(chats);
    } catch (error) {
      console.error('Failed to load contacts:', error);
    } finally {
      setLoading(false);
    }
  }

  async function handleAddContact(contact: Contact) {
    try {
      await api.addContact(contact.peer_id, contact.name ?? undefined);
      await loadContacts();
    } catch (error) {
      alert('Failed to add contact: ' + error);
    }
  }

  async function handleDhtSearch(e: React.FormEvent) {
    e.preventDefault();
    if (!dhtSearchId.trim()) return;
    setDhtSearching(true);
    try {
      await api.findPeer(dhtSearchId.trim());
      // Result comes via peer-discovered event — contacts will refresh
      setTimeout(() => {
        loadContacts();
        setDhtSearching(false);
      }, 3000);
    } catch (error) {
      alert('DHT search failed: ' + error);
      setDhtSearching(false);
    }
  }

  function startEditing(contact: Contact, e: React.MouseEvent) {
    e.stopPropagation();
    setEditingPeerId(contact.peer_id);
    setEditingName(contact.name ?? '');
    setTimeout(() => editInputRef.current?.focus(), 0);
  }

  async function commitRename(peerId: string) {
    try {
      const trimmed = editingName.trim();
      await api.renameContact(peerId, trimmed || null);
      await loadContacts();
    } catch (error) {
      console.error('Failed to rename contact:', error);
    } finally {
      setEditingPeerId(null);
    }
  }

  function handleRenameKeyDown(e: React.KeyboardEvent, peerId: string) {
    if (e.key === 'Enter') { e.preventDefault(); commitRename(peerId); }
    if (e.key === 'Escape') { setEditingPeerId(null); }
  }

  // Listen for peer-discovered to refresh after DHT search
  useEffect(() => {
    const unlisten = listen('peer-discovered', () => {
      loadContacts();
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Reload once network/AppState is ready (in case initial load happened before AppState registered)
  useEffect(() => {
    const unlisten = listen('listen-addr-added', () => {
      loadContacts();
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Track online peers
  useEffect(() => {
    const unlistenConnected = listen<{ peer_id: string }>('peer-connected', (event) => {
      setOnlinePeers(prev => new Set(prev).add(event.payload.peer_id));
    });
    const unlistenDisconnected = listen<{ peer_id: string }>('peer-disconnected', (event) => {
      setOnlinePeers(prev => {
        const next = new Set(prev);
        next.delete(event.payload.peer_id);
        return next;
      });
    });
    return () => {
      unlistenConnected.then(fn => fn());
      unlistenDisconnected.then(fn => fn());
    };
  }, []);

  function formatPeerId(peerId: string): string {
    if (peerId.length > 20) {
      return `${peerId.slice(0, 8)}...${peerId.slice(-8)}`;
    }
    return peerId;
  }

  const sortByOnlineThenLastSeen = (list: Contact[]) =>
    [...list].sort((a, b) => {
      const aOnline = onlinePeers.has(a.peer_id) ? 1 : 0;
      const bOnline = onlinePeers.has(b.peer_id) ? 1 : 0;
      if (aOnline !== bOnline) return bOnline - aOnline;
      return (b.last_seen ?? 0) - (a.last_seen ?? 0);
    });

  const chatPeerIds = new Set(chatPeers.map(c => c.peer_id));

  // Contacts tab: chat history + manually added (even without messages)
  const manualWithoutChat = contacts.filter(c => c.is_manual && !chatPeerIds.has(c.peer_id));
  const manual = sortByOnlineThenLastSeen([...chatPeers, ...manualWithoutChat]);

  // Strangers: connected but no messages and not manually added
  const strangers = sortByOnlineThenLastSeen(
    contacts.filter(c => !c.is_manual && !chatPeerIds.has(c.peer_id))
  );

  const filterFn = (c: Contact) => {
    if (!searchQuery.trim()) return true;
    const q = searchQuery.toLowerCase();
    return (
      c.peer_id.toLowerCase().includes(q) ||
      (c.name ?? '').toLowerCase().includes(q)
    );
  };

  const visibleContacts = (tab === 'contacts' ? manual : strangers).filter(filterFn);

  if (loading) {
    return <div style={{ padding: '16px' }}>Loading contacts...</div>;
  }

  return (
    <div className="contact-list">
      <div className="contact-tabs">
        <button
          className={`tab-btn ${tab === 'contacts' ? 'active' : ''}`}
          onClick={() => setTab('contacts')}
        >
          Contacts {manual.length > 0 && `(${manual.length})`}
        </button>
        <button
          className={`tab-btn ${tab === 'strangers' ? 'active' : ''}`}
          onClick={() => setTab('strangers')}
        >
          Strangers {strangers.length > 0 && `(${strangers.length})`}
        </button>
      </div>

      <div className="contact-search">
        <input
          type="text"
          placeholder="Filter by name or ID..."
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
          className="search-input"
        />
      </div>

      <div className="contact-items">
        {visibleContacts.length === 0 ? (
          <div style={{ padding: '16px', color: '#666' }}>
            {tab === 'contacts' ? 'No contacts yet.' : 'No strangers seen yet.'}
          </div>
        ) : (
          visibleContacts.map((contact) => (
            <div
              key={contact.peer_id}
              className={`contact-item ${selectedContact?.peer_id === contact.peer_id ? 'active' : ''}`}
              onClick={() => onSelectContact(contact)}
            >
              <div className="contact-item-info">
                {editingPeerId === contact.peer_id ? (
                  <input
                    ref={editInputRef}
                    className="contact-name-input"
                    value={editingName}
                    onChange={e => setEditingName(e.target.value)}
                    onBlur={() => commitRename(contact.peer_id)}
                    onKeyDown={e => handleRenameKeyDown(e, contact.peer_id)}
                    onClick={e => e.stopPropagation()}
                  />
                ) : (
                  <div className="contact-name">
                    <span
                      className={`online-dot ${onlinePeers.has(contact.peer_id) ? 'online' : 'offline'}`}
                    />
                    {contact.name || 'Unknown'}
                    {tab === 'contacts' && (
                      <button
                        className="rename-btn"
                        onClick={e => startEditing(contact, e)}
                        title="Rename"
                      >
                        ✎
                      </button>
                    )}
                  </div>
                )}
                <div className="contact-peer-id">
                  {formatPeerId(contact.peer_id)}
                </div>
              </div>
              {tab === 'strangers' && (
                <button
                  className="add-stranger-btn"
                  onClick={e => { e.stopPropagation(); handleAddContact(contact); }}
                  title="Add to contacts"
                >
                  +
                </button>
              )}
            </div>
          ))
        )}
      </div>

      <div className="dht-search">
        <form onSubmit={handleDhtSearch}>
          <input
            type="text"
            placeholder="Find peer by ID in DHT..."
            value={dhtSearchId}
            onChange={e => setDhtSearchId(e.target.value)}
            className="search-input"
          />
          <button type="submit" disabled={dhtSearching} style={{ marginTop: '6px', width: '100%' }}>
            {dhtSearching ? 'Searching...' : 'Search DHT'}
          </button>
        </form>
      </div>
    </div>
  );
}
