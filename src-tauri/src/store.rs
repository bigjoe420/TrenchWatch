//! Append-oriented local event store (docs/ARCHITECTURE.md §2).
//!
//! Every record is written as a versioned JSON envelope with a CRC32 of
//! its payload. On open, every line is re-verified; a mismatch on a
//! current-format record means tampering or corruption and the store
//! refuses to serve data (docs/THREAT_MODEL.md §6) rather than risk
//! showing someone a fabricated history. Legacy-format records whose
//! payload still parses are re-sealed under the current format instead of
//! bricking the store on every schema addition — after that one-time
//! migration, verification is strict again.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::listener::LaunchEvent;
use crate::risk::{Graduation, LaunchAnalysis};

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

/// Envelope format versions. v1 = CRC over the plain struct-order
/// serialization (what older builds wrote). v2 = CRC over the canonical
/// sorted-key JSON of the payload, which is stable when fields are added to
/// a stored struct (a new `#[serde(default)]` field must not brick old
/// records).
const SCHEMA_LEGACY: u32 = 1;
const SCHEMA_CANONICAL: u32 = 2;
const SCHEMA_CURRENT: u32 = SCHEMA_CANONICAL;

fn schema_default() -> u32 {
    SCHEMA_LEGACY
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope<T> {
    crc32: u32,
    #[serde(default = "schema_default")]
    schema: u32,
    payload: T,
}

/// Canonical form: sorted keys at every level, independent of struct field
/// order and of schema additions.
fn canonical_bytes<T: Serialize>(payload: &T) -> Result<Vec<u8>, StoreError> {
    let value = serde_json::to_value(payload).map_err(|e| StoreError::Serde(e.to_string()))?;
    serde_json::to_vec(&value).map_err(|e| StoreError::Serde(e.to_string()))
}

impl<T: Serialize + Clone> Envelope<T> {
    fn wrap(payload: &T) -> Result<Self, StoreError> {
        let bytes = canonical_bytes(payload)?;
        Ok(Envelope {
            crc32: crc32fast::hash(&bytes),
            schema: SCHEMA_CURRENT,
            payload: payload.clone(),
        })
    }
}

impl<T: Serialize + DeserializeOwned> Envelope<T> {
    /// Strict verification within the line's own format version.
    fn verify(&self) -> Result<(), StoreError> {
        let bytes = if self.schema >= SCHEMA_CANONICAL {
            canonical_bytes(&self.payload)?
        } else {
            serde_json::to_vec(&self.payload).map_err(|e| StoreError::Serde(e.to_string()))?
        };
        if crc32fast::hash(&bytes) != self.crc32 {
            return Err(StoreError::Checksum(0));
        }
        Ok(())
    }
}

fn load_verified<T: Serialize + DeserializeOwned + Clone>(
    path: &Path,
) -> Result<Vec<T>, StoreError> {
    let mut records = Vec::new();
    if !path.exists() {
        return Ok(records);
    }
    let reader = BufReader::new(File::open(path)?);
    let mut needs_reseal = false;
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let envelope: Envelope<T> =
            serde_json::from_str(&line).map_err(|_| StoreError::Checksum(index + 1))?;
        if envelope.verify().is_err() {
            if envelope.schema >= SCHEMA_CURRENT {
                // Current-format record that fails verification: corruption
                // or tampering — refuse to serve it (THREAT_MODEL.md §6).
                return Err(StoreError::Checksum(index + 1));
            }
            // Legacy record: a schema addition changes the round-trip bytes,
            // so the original CRC can no longer be reproduced. The payload
            // itself still parses under the current schema; accept it and
            // re-seal the whole file under the current format.
            needs_reseal = true;
        }
        records.push(envelope.payload);
    }
    if needs_reseal {
        let mut file = File::create(path)?;
        for record in &records {
            append_line(&mut file, record)?;
        }
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
        // Analyses are append-only: a re-assessment (or a later lifecycle
        // update like graduation) arrives as a new line for an existing
        // signature. On load, the LATEST record per signature wins —
        // serving the first (stale) copy would silently undo updates.
        let mut analyses: Vec<LaunchAnalysis> = Vec::new();
        for analysis in load_verified::<LaunchAnalysis>(&analyses_path)? {
            match analyses
                .iter()
                .position(|existing| existing.signature == analysis.signature)
            {
                Some(position) => analyses[position] = analysis,
                None => analyses.push(analysis),
            }
        }

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

    /// Every stored analysis attributed to `wallet`, excluding the analysis
    /// for `exclude_signature` (the launch currently being assessed). Feeds
    /// the local reputation signal — derived from our own receipts only.
    pub fn analyses_for_wallet(
        &self,
        wallet: &str,
        exclude_signature: &str,
    ) -> Vec<LaunchAnalysis> {
        self.analyses
            .iter()
            .filter(|analysis| {
                analysis.dev_wallet.as_deref() == Some(wallet)
                    && analysis.signature != exclude_signature
            })
            .cloned()
            .collect()
    }

    /// Signature of the earliest stored launch on `program` whose analysis
    /// identified `mint` — the handle used to link a token's life stages.
    /// Graduation linking calls this with the Pump.fun program id.
    pub fn launch_signature_by_mint(&self, mint: &str, program: &str) -> Option<String> {
        self.events
            .iter()
            .filter(|event| event.program == program)
            .filter_map(|event| self.analysis_for(&event.signature))
            .find(|analysis| analysis.mint.as_deref() == Some(mint))
            .map(|analysis| analysis.signature.clone())
    }

    /// Attach a graduation to the launch `launch_signature` and persist the
    /// updated analysis (same replace-by-signature append as a re-assessment).
    /// Returns the updated analysis when the launch is known.
    pub fn mark_graduated(
        &mut self,
        launch_signature: &str,
        graduation: Graduation,
    ) -> Option<LaunchAnalysis> {
        let mut updated = self.analysis_for(launch_signature)?.clone();
        updated.graduation = Some(graduation);
        self.append_analysis(&updated).ok()?;
        Some(updated)
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
            dev_wallet: None,
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

    #[test]
    fn pumpswap_pool_links_to_earlier_pumpfun_launch() {
        let dir = std::env::temp_dir().join(format!("tw-store-grad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let pumpfun_event = LaunchEvent {
            signature: "pumpfun-sig".to_string(),
            program: crate::listener::PUMPFUN_PROGRAM.to_string(),
            program_label: "Pump.fun".to_string(),
            ..sample_event()
        };
        let pumpswap_event = LaunchEvent {
            signature: "migration-sig".to_string(),
            slot: 100,
            program: crate::listener::PUMPSWAP_PROGRAM.to_string(),
            program_label: "PumpSwap".to_string(),
            ..sample_event()
        };

        {
            let mut store = EventStore::open(&dir).unwrap();
            store.append(&pumpfun_event).unwrap();
            store.append(&pumpswap_event).unwrap();

            // Both launches analyze to the same mint (the token).
            let mut launch_analysis = sample_analysis();
            launch_analysis.signature = "pumpfun-sig".to_string();
            store.append_analysis(&launch_analysis).unwrap();
            let mut pool_analysis = sample_analysis();
            pool_analysis.signature = "migration-sig".to_string();
            store.append_analysis(&pool_analysis).unwrap();

            // The PumpSwap pool creation IS the graduation of the pump.fun
            // launch: link them by mint and persist the updated analysis.
            let launch_signature = store
                .launch_signature_by_mint("mint111", crate::listener::PUMPFUN_PROGRAM)
                .expect("pump.fun launch should be found by mint");
            assert_eq!(launch_signature, "pumpfun-sig");

            let updated = store
                .mark_graduated(
                    &launch_signature,
                    crate::risk::Graduation {
                        signature: "migration-sig".to_string(),
                        slot: 100,
                        venue_label: "PumpSwap".to_string(),
                        detected_unix_ms: 1_700_000_001_000,
                    },
                )
                .expect("graduation should attach");
            assert_eq!(updated.graduation.as_ref().map(|g| g.slot), Some(100));
        }
        {
            // The graduation must survive a reload (old-version lines in the
            // analyses file are tolerated by design).
            let store = EventStore::open(&dir).unwrap();
            let analysis = store.analysis_for("pumpfun-sig").unwrap();
            let graduation = analysis.graduation.as_ref().expect("graduation persists");
            assert_eq!(graduation.signature, "migration-sig");
            assert_eq!(graduation.venue_label, "PumpSwap");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_link_when_mint_unknown_or_launch_unseen() {
        let dir = std::env::temp_dir().join(format!("tw-store-nolink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut store = EventStore::open(&dir).unwrap();
        // No events at all → nothing to link.
        assert!(store
            .launch_signature_by_mint("mint111", crate::listener::PUMPFUN_PROGRAM)
            .is_none());
        // Unknown launch → mark_graduated returns None, store stays intact.
        assert!(store
            .mark_graduated(
                "missing-sig",
                crate::risk::Graduation {
                    signature: "migration-sig".to_string(),
                    slot: 1,
                    venue_label: "PumpSwap".to_string(),
                    detected_unix_ms: 1,
                },
            )
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn analysis_with_float_evidence_survives_reload_verification() {
        let dir = std::env::temp_dir().join(format!("tw-store-floats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Percentages and ratios are f64 in flag evidence. Without
        // serde_json's float_roundtrip feature, re-parsing such values can
        // land on a neighboring f64 whose re-serialization differs from the
        // sealed text — which the store's CRC then (correctly) rejects as
        // tampering on the next boot. These values are chosen to trip the
        // imprecise fast-parse path; the test fails without the feature.
        let mut analysis = sample_analysis();
        analysis.signature = "float-sig".to_string();
        analysis.flags.push(crate::risk::RiskFlag {
            signal: SignalKind::HolderConcentration,
            severity: crate::risk::Severity::Warn,
            explanation: "float evidence regression test".to_string(),
            evidence: serde_json::json!({
                "pct_of_supply": 33.333333333333336_f64,
                "ratio": 2.675_f64,
                "largest": 0.1_f64,
            }),
            field_guide_ref: "docs/FIELD_GUIDE.md#test".to_string(),
            slot: 42,
            observed_unix_ms: 1_700_000_000_000,
        });

        {
            let mut store = EventStore::open(&dir).unwrap();
            store.append_analysis(&analysis).unwrap();
        }
        {
            let store = EventStore::open(&dir).unwrap();
            let loaded = store.analysis_for("float-sig").expect("must verify and load");
            let float_flag = loaded
                .flags
                .iter()
                .find(|flag| flag.signal == SignalKind::HolderConcentration)
                .expect("float flag must be present");
            assert_eq!(float_flag.evidence["pct_of_supply"], 33.333333333333336_f64);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_record_from_before_schema_addition_still_verifies() {        let dir = std::env::temp_dir().join(format!("tw-store-v2old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // A schema-2 record written by a build that predates the graduation
        // field: payload JSON without that key, CRC over the canonical
        // sorted-key bytes — exactly what is on disk in the field. Adding a
        // field to a stored struct must not fail verification (store.rs
        // format note), because serde-defaulted fields that serialize to
        // nothing keep the round-trip bytes identical.
        let old_payload = serde_json::json!({
            "assessed_at_unix_ms": 1_700_000_000_000_u64,
            "dev_wallet": null,
            "flags": [],
            "mint": null,
            "not_assessed": [],
            "signature": "5ig5ig5ig",
            "slot": 42,
        });
        let canonical = serde_json::to_vec(&old_payload).unwrap();
        let line = serde_json::json!({
            "crc32": crc32fast::hash(&canonical),
            "payload": old_payload,
            "schema": 2,
        });
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ANALYSES_FILE), format!("{line}\n")).unwrap();

        let store = EventStore::open(&dir).unwrap();
        let analyses = store.recent_analyses(10);
        assert_eq!(analyses.len(), 1);
        assert!(analyses[0].graduation.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_record_is_resealed_not_rejected() {
        let dir = std::env::temp_dir().join(format!("tw-store-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("store")).unwrap();

        // Simulate a record written by an older build: payload without the
        // dev_wallet field, no schema marker, CRC over the plain
        // serialization — exactly what a schema addition would otherwise
        // reject as "tampering".
        let legacy_payload = serde_json::json!({
            "signature": "5ig5ig5ig",
            "slot": 42,
            "mint": null,
            "flags": [],
            "not_assessed": [],
            "assessed_at_unix_ms": 1_700_000_000_000_u64,
        });
        let payload_bytes = serde_json::to_vec(&legacy_payload).unwrap();
        let line = serde_json::json!({
            "crc32": crc32fast::hash(&payload_bytes),
            "payload": legacy_payload,
        });
        std::fs::write(dir.join(ANALYSES_FILE), format!("{line}\n")).unwrap();

        {
            let store = EventStore::open(&dir).unwrap();
            let analyses = store.recent_analyses(10);
            assert_eq!(analyses.len(), 1);
            assert_eq!(analyses[0].dev_wallet, None);
        }
        // The file was re-sealed under the current format: reopening is a
        // strict verification, not another migration.
        let resealed = std::fs::read_to_string(dir.join(ANALYSES_FILE)).unwrap();
        assert!(resealed.contains("\"schema\":2"));
        let store = EventStore::open(&dir).unwrap();
        assert_eq!(store.recent_analyses(10).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
