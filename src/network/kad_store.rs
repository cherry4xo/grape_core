use libp2p::kad::{
    store::{RecordStore, Result as StoreResult, Error as StoreError},
    Record, ProviderRecord, RecordKey as Key,
};
use libp2p::PeerId;
use sled::Db;
use std::borrow::Cow;
use tracing::{debug, warn};

/// Persistent Kademlia record store using sled database
pub struct PersistentStore {
    db: Db,
    local_peer_id: PeerId,
    max_records: usize,
    max_record_size: usize,
    max_providers_per_key: usize,
}

impl PersistentStore {
    pub fn new(path: std::path::PathBuf, local_peer_id: PeerId) -> Result<Self, sled::Error> {
        let db = sled::open(path)?;

        Ok(Self {
            db,
            local_peer_id,
            max_records: 1024,
            max_record_size: 64 * 1024, // 64 KB
            max_providers_per_key: 20,
        })
    }

    fn record_key(&self, key: &Key) -> Vec<u8> {
        let mut result = b"record:".to_vec();
        result.extend_from_slice(key.as_ref());
        result
    }

    fn provider_key(&self, key: &Key, provider: &PeerId) -> Vec<u8> {
        let mut result = b"provider:".to_vec();
        result.extend_from_slice(key.as_ref());
        result.push(b':');
        result.extend_from_slice(&provider.to_bytes());
        result
    }

    fn provider_prefix(&self, key: &Key) -> Vec<u8> {
        let mut result = b"provider:".to_vec();
        result.extend_from_slice(key.as_ref());
        result.push(b':');
        result
    }

    /// Serialize Record: Key length (4 bytes) + Key + Value length (4 bytes) + Value + Publisher (optional)
    fn serialize_record(&self, record: &Record) -> Vec<u8> {
        let mut data = Vec::new();

        // Key
        let key_bytes = record.key.as_ref();
        data.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
        data.extend_from_slice(key_bytes);

        // Value
        data.extend_from_slice(&(record.value.len() as u32).to_be_bytes());
        data.extend_from_slice(&record.value);

        // Publisher (optional)
        if let Some(publisher) = &record.publisher {
            data.push(1u8); // has publisher
            data.extend_from_slice(&publisher.to_bytes());
        } else {
            data.push(0u8); // no publisher
        }

        data
    }

    fn deserialize_record(&self, data: &[u8]) -> Option<Record> {
        let mut offset = 0;

        // Key
        if data.len() < offset + 4 {
            return None;
        }
        let key_len = u32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        offset += 4;

        if data.len() < offset + key_len {
            return None;
        }
        let key = Key::from(data[offset..offset + key_len].to_vec());
        offset += key_len;

        // Value
        if data.len() < offset + 4 {
            return None;
        }
        let value_len = u32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        offset += 4;

        if data.len() < offset + value_len {
            return None;
        }
        let value = data[offset..offset + value_len].to_vec();
        offset += value_len;

        // Publisher
        if data.len() < offset + 1 {
            return None;
        }
        let has_publisher = data[offset] == 1;
        offset += 1;

        let publisher = if has_publisher {
            if data.len() < offset + 38 {
                return None;
            }
            let peer_bytes = &data[offset..offset + 38];
            PeerId::from_bytes(peer_bytes).ok()
        } else {
            None
        };

        Some(Record {
            key,
            value,
            publisher,
            expires: None, // Don't persist expiration time
        })
    }

    /// Serialize ProviderRecord: Key + Provider (don't persist expires/addresses)
    fn serialize_provider(&self, record: &ProviderRecord) -> Vec<u8> {
        let mut data = Vec::new();

        // Key
        let key_bytes = record.key.as_ref();
        data.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
        data.extend_from_slice(key_bytes);

        // Provider
        data.extend_from_slice(&record.provider.to_bytes());

        data
    }

    fn deserialize_provider(&self, data: &[u8]) -> Option<ProviderRecord> {
        let mut offset = 0;

        // Key
        if data.len() < offset + 4 {
            return None;
        }
        let key_len = u32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        offset += 4;

        if data.len() < offset + key_len {
            return None;
        }
        let key = Key::from(data[offset..offset + key_len].to_vec());
        offset += key_len;

        // Provider
        if data.len() < offset + 38 {
            return None;
        }
        let provider = PeerId::from_bytes(&data[offset..offset + 38]).ok()?;

        Some(ProviderRecord {
            key,
            provider,
            expires: None, // Don't persist expiration time
            addresses: Vec::new(), // Don't persist addresses
        })
    }
}

impl RecordStore for PersistentStore {
    type RecordsIter<'a> = std::vec::IntoIter<Cow<'a, Record>>;
    type ProvidedIter<'a> = std::vec::IntoIter<Cow<'a, ProviderRecord>>;

    fn get(&self, key: &Key) -> Option<Cow<'_, Record>> {
        let db_key = self.record_key(key);

        match self.db.get(&db_key) {
            Ok(Some(data)) => {
                self.deserialize_record(&data).map(|r| {
                    debug!("Retrieved record for key: {:?}", key);
                    Cow::Owned(r)
                })
            }
            Ok(None) => None,
            Err(e) => {
                warn!("Failed to get record from store: {:?}", e);
                None
            }
        }
    }

    fn put(&mut self, record: Record) -> StoreResult<()> {
        if record.value.len() > self.max_record_size {
            return Err(StoreError::ValueTooLarge);
        }

        let key = self.record_key(&record.key);
        let data = self.serialize_record(&record);

        match self.db.insert(key, data) {
            Ok(_) => {
                debug!("Stored record for key: {:?}", record.key);
                let _ = self.db.flush();
                Ok(())
            }
            Err(e) => {
                warn!("Failed to store record: {:?}", e);
                Err(StoreError::MaxRecords)
            }
        }
    }

    fn remove(&mut self, key: &Key) {
        let db_key = self.record_key(key);

        if let Err(e) = self.db.remove(&db_key) {
            warn!("Failed to remove record: {:?}", e);
        } else {
            debug!("Removed record for key: {:?}", key);
            let _ = self.db.flush();
        }
    }

    fn records(&self) -> Self::RecordsIter<'_> {
        let mut records = Vec::new();

        for item in self.db.scan_prefix(b"record:") {
            if let Ok((_, value)) = item {
                if let Some(record) = self.deserialize_record(&value) {
                    records.push(Cow::Owned(record));
                }
            }
        }

        records.into_iter()
    }

    fn add_provider(&mut self, record: ProviderRecord) -> StoreResult<()> {
        let key = self.provider_key(&record.key, &record.provider);

        // Check if we already have too many providers
        let prefix = self.provider_prefix(&record.key);
        let provider_count = self.db.scan_prefix(&prefix).count();

        if provider_count >= self.max_providers_per_key {
            return Err(StoreError::MaxProvidedKeys);
        }

        let data = self.serialize_provider(&record);

        match self.db.insert(key, data) {
            Ok(_) => {
                debug!("Stored provider {:?} for key: {:?}", record.provider, record.key);
                let _ = self.db.flush();
                Ok(())
            }
            Err(e) => {
                warn!("Failed to store provider: {:?}", e);
                Err(StoreError::MaxProvidedKeys)
            }
        }
    }

    fn providers(&self, key: &Key) -> Vec<ProviderRecord> {
        let prefix = self.provider_prefix(key);
        let mut providers = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            if let Ok((_, value)) = item {
                if let Some(record) = self.deserialize_provider(&value) {
                    providers.push(record);
                }
            }
        }

        providers
    }

    fn provided(&self) -> Self::ProvidedIter<'_> {
        let mut provided = Vec::new();

        for item in self.db.scan_prefix(b"provider:") {
            if let Ok((_, value)) = item {
                if let Some(record) = self.deserialize_provider(&value) {
                    if record.provider == self.local_peer_id {
                        provided.push(Cow::Owned(record));
                    }
                }
            }
        }

        provided.into_iter()
    }

    fn remove_provider(&mut self, key: &Key, provider: &PeerId) {
        let db_key = self.provider_key(key, provider);

        if let Err(e) = self.db.remove(&db_key) {
            warn!("Failed to remove provider: {:?}", e);
        } else {
            debug!("Removed provider {:?} for key: {:?}", provider, key);
            let _ = self.db.flush();
        }
    }
}
