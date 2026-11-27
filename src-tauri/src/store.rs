//! Append-oriented local event store (docs/ARCHITECTURE.md §2).
//!
//! Every event is written as a JSON envelope with a CRC32 of its payload.
//! On open, every line is re-verified; a mismatch means tampering or
//! corruption and the store refuses to serve data (docs/THREAT_MODEL.md §6)
//! rather than risk showing someone a fabricated history.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::listener::LaunchEvent;

const STORE_FILE: &str = "events.jsonl";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store checksum mismatch at line {0} — refusing to serve possibly tampered data")]
    Checksum(usize),
    #[error("store is locked by another component")]
    Poisoned,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("store serialization error: {0}")]
    Serde(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    crc32: u32,
    event: LaunchEvent,
}

impl Envelope {
    fn wrap(event: &LaunchEvent) -> Result<Self, StoreError> {
        let payload = serde_json::to_vec(event).map_err(|e| StoreError::Serde(e.to_string()))?;
        Ok(Envelope {
            crc32: crc32fast::hash(&payload),
            event: event.clone(),
        })
    }

    fn verify(&self) -> Result<(), StoreError> {
        let payload =
            serde_json::to_vec(&self.event).map_err(|e| StoreError::Serde(e.to_string()))?;
        if crc32fast::hash(&payload) != self.crc32 {
            return Err(StoreError::Checksum(0));
        }
        Ok(())
    }
}

pub struct EventStore {
    path: PathBuf,
    file: File,
    events: Vec<LaunchEvent>,
}

impl EventStore {
    /// Open (or create) the store, verifying every existing record.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(STORE_FILE);

        let mut events = Vec::new();
        if path.exists() {
            let reader = BufReader::new(File::open(&path)?);
            for (index, line) in reader.lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let envelope: Envelope =
                    serde_json::from_str(&line).map_err(|_| StoreError::Checksum(index + 1))?;
                envelope
                    .verify()
                    .map_err(|_| StoreError::Checksum(index + 1))?;
                events.push(envelope.event);
            }
        }

        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { path, file, events })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, event: &LaunchEvent) -> Result<(), StoreError> {
        let envelope = Envelope::wrap(event)?;
        let line =
            serde_json::to_string(&envelope).map_err(|e| StoreError::Serde(e.to_string()))?;
        writeln!(self.file, "{line}")?;
        self.file.flush()?;
        self.events.push(event.clone());
        Ok(())
    }

    /// Newest-first tail, for the dashboard.
    pub fn recent(&self, limit: usize) -> Vec<LaunchEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> LaunchEvent {
        LaunchEvent {
            signature: "5ig".repeat(10),
            slot: 42,
            program: "prog".to_string(),
            program_label: "Test Program".to_string(),
            logs: vec!["log line".to_string()],
            detected_unix_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn roundtrip_persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("tw-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut store = EventStore::open(&dir).unwrap();
            store.append(&sample_event()).unwrap();
            assert_eq!(store.len(), 1);
        }
        {
            let store = EventStore::open(&dir).unwrap();
            let recent = store.recent(10);
            assert_eq!(recent.len(), 1);
            assert_eq!(recent[0].slot, 42);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_line_is_rejected() {
        let dir = std::env::temp_dir().join(format!("tw-store-tamper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut store = EventStore::open(&dir).unwrap();
            store.append(&sample_event()).unwrap();
        }
        // Change one digit in the stored line (JSON stays valid, CRC no
        // longer matches the payload).
        let path = dir.join(STORE_FILE);
        let mut bytes = std::fs::read(&path).unwrap();
        let pos = bytes
            .iter()
            .rposition(|byte| byte.is_ascii_digit())
            .unwrap();
        bytes[pos] = if bytes[pos] == b'9' {
            b'0'
        } else {
            bytes[pos] + 1
        };
        std::fs::write(&path, bytes).unwrap();

        assert!(matches!(
            EventStore::open(&dir),
            Err(StoreError::Checksum(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
