//! Append-oriented local event store (docs/ARCHITECTURE.md §2).
//!
//! Every record is written as a JSON envelope with a CRC32 of its payload.
//! On open, every line is re-verified; a mismatch means tampering or
//! corruption and the store refuses to serve data (docs/THREAT_MODEL.md §6)
//! rather than risk showing someone a fabricated history.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::listener::LaunchEvent;
use crate::risk::LaunchAnalysis;

const EVENTS_FILE: &str = "events.jsonl";
const ANALYSES_FILE: &str = "analyses.jsonl";

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
struct Envelope<T> {
    crc32: u32,
    payload: T,
}

impl<T: Serialize + Clone> Envelope<T> {
    fn wrap(payload: &T) -> Result<Self, StoreError> {
        let bytes = serde_json::to_vec(payload).map_err(|e| StoreError::Serde(e.to_string()))?;
        Ok(Envelope {
            crc32: crc32fast::hash(&bytes),
            payload: payload.clone(),
        })
    }
}

impl<T: Serialize + DeserializeOwned> Envelope<T> {
    fn verify(&self) -> Result<(), StoreError> {
        let bytes =
            serde_json::to_vec(&self.payload).map_err(|e| StoreError::Serde(e.to_string()))?;
        if crc32fast::hash(&bytes) != self.crc32 {
            return Err(StoreError::Checksum(0));
        }
        Ok(())
    }
}

fn load_verified<T: Serialize + DeserializeOwned>(path: &Path) -> Result<Vec<T>, StoreError> {
    let mut records = Vec::new();
    if !path.exists() {
        return Ok(records);
    }
    let reader = BufReader::new(File::open(path)?);
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let envelope: Envelope<T> =
            serde_json::from_str(&line).map_err(|_| StoreError::Checksum(index + 1))?;
        envelope
            .verify()
            .map_err(|_| StoreError::Checksum(index + 1))?;
        records.push(envelope.payload);
    }
    Ok(records)
}

fn append_line<T: Serialize + Clone>(file: &mut File, payload: &T) -> Result<(), StoreError> {
    let envelope = Envelope::wrap(payload)?;
    let line = serde_json::to_string(&envelope).map_err(|e| StoreError::Serde(e.to_string()))?;
    writeln!(file, "{line}")?;
    file.flush()?;
    Ok(())
}

pub struct EventStore {
    events_path: PathBuf,
    events_file: File,
    analyses_file: File,
    events: Vec<LaunchEvent>,
    analyses: Vec<LaunchAnalysis>,
}

impl EventStore {
    /// Open (or create) the store, verifying every existing record.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir)?;
        let events_path = dir.join(EVENTS_FILE);
        let analyses_path = dir.join(ANALYSES_FILE);

        let events = load_verified(&events_path)?;
        let analyses = load_verified(&analyses_path)?;

        let events_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_path)?;
        let analyses_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&analyses_path)?;
        Ok(Self {
            events_path,
            events_file,
            analyses_file,
            events,
            analyses,
        })
    }

    pub fn path(&self) -> &Path {
        &self.events_path
    }

    pub fn append(&mut self, event: &LaunchEvent) -> Result<(), StoreError> {
        append_line(&mut self.events_file, event)?;
        self.events.push(event.clone());
        Ok(())
    }

    pub fn append_analysis(&mut self, analysis: &LaunchAnalysis) -> Result<(), StoreError> {
        // One analysis per launch — replace any earlier copy in memory.
        self.analyses
            .retain(|existing| existing.signature != analysis.signature);
        append_line(&mut self.analyses_file, analysis)?;
        self.analyses.push(analysis.clone());
        Ok(())
    }

    /// Newest-first tail, for the dashboard.
    pub fn recent(&self, limit: usize) -> Vec<LaunchEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    /// Newest-first analyses, for the dashboard.
    pub fn recent_analyses(&self, limit: usize) -> Vec<LaunchAnalysis> {
        self.analyses.iter().rev().take(limit).cloned().collect()
    }

    pub fn analysis_for(&self, signature: &str) -> Option<&LaunchAnalysis> {
        self.analyses
            .iter()
            .find(|analysis| analysis.signature == signature)
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
    use crate::provider::MintAccount;
    use crate::risk::{assess, AnalysisContext, AnalysisInput, HolderSnapshot, SignalKind};

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

    fn sample_analysis() -> LaunchAnalysis {
        let input = AnalysisInput {
            mint_address: "mint111".to_string(),
            mint: MintAccount {
                supply_raw: 1_000_000,
                decimals: 6,
                mint_authority: Some("auth".to_string()),
                freeze_authority: None,
            },
            holders: vec![HolderSnapshot {
                address: "holder".to_string(),
                amount_raw: 500_000,
            }],
        };
        assess(
            Some(&input),
            &AnalysisContext {
                signature: "5ig5ig5ig",
                slot: 42,
                observed_unix_ms: 1_700_000_000_000,
            },
            vec![],
        )
    }

    #[test]
    fn roundtrip_persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("tw-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut store = EventStore::open(&dir).unwrap();
            store.append(&sample_event()).unwrap();
            store.append_analysis(&sample_analysis()).unwrap();
            assert_eq!(store.len(), 1);
        }
        {
            let store = EventStore::open(&dir).unwrap();
            assert_eq!(store.recent(10).len(), 1);
            let analyses = store.recent_analyses(10);
            assert_eq!(analyses.len(), 1);
            assert!(analyses[0]
                .flags
                .iter()
                .any(|flag| flag.signal == SignalKind::MintFreezeAuthority));
            assert!(store.analysis_for("5ig5ig5ig").is_some());
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
        let path = dir.join(EVENTS_FILE);
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
