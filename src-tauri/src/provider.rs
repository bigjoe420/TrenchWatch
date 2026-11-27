//! The only RPC boundary in the app (docs/ARCHITECTURE.md §2).
//!
//! Read-only by design (docs/THREAT_MODEL.md §2): requests are built
//! through [`RpcRequest`], which refuses any method outside
//! [`ALLOWED_METHODS`]. There is deliberately no generic passthrough — a
//! future contributor cannot accidentally route `sendTransaction` through
//! here without hitting the whitelist and the unit test that guards it.
//!
//! The [`ChainProvider`] trait is the seam where a Helius-backed transport
//! (API key, webhooks) drops in later with zero changes to callers.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

/// Read-only JSON-RPC methods this app may call. Anything else is rejected
/// at request construction time, not at review time.
pub const ALLOWED_METHODS: &[&str] = &[
    "getAccountInfo",
    "getProgramAccounts",
    "getSignaturesForAddress",
    "getSlot",
    "getTokenAccountBalance",
    "getTokenLargestAccounts",
    "getTokenSupply",
    "getTransaction",
    "logsSubscribe",
    "logsUnsubscribe",
];

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("method '{0}' is not on the read-only whitelist")]
    MethodNotAllowed(String),
    #[error("transport error: {0}")]
    Transport(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("endpoint returned an error: {0}")]
    Rpc(String),
    #[error("protocol violation: {0}")]
    Protocol(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for ProviderError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        ProviderError::Transport(Box::new(error))
    }
}

/// A JSON-RPC request whose method is guaranteed to be on the whitelist.
#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub id: u64,
    pub method: &'static str,
    pub params: Value,
}

impl RpcRequest {
    pub fn new(id: u64, method: &str, params: Value) -> Result<Self, ProviderError> {
        let method = ALLOWED_METHODS
            .iter()
            .find(|allowed| **allowed == method)
            .copied()
            .ok_or_else(|| ProviderError::MethodNotAllowed(method.to_string()))?;
        Ok(Self { id, method, params })
    }

    fn to_json(&self) -> Value {
        json!({ "jsonrpc": "2.0", "id": self.id, "method": self.method, "params": self.params })
    }
}

/// A normalized pool-program log notification, transport-independent.
#[derive(Debug, Clone, Serialize)]
pub struct LogNotification {
    pub signature: String,
    pub slot: u64,
    pub program: String,
    pub logs: Vec<String>,
    /// Raw server-side subscription id (diagnostics).
    #[serde(skip)]
    pub subscription_id: u64,
}

/// An SPL-token mint account, decoded from raw account data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MintAccount {
    pub supply_raw: u64,
    pub decimals: u8,
    /// `None` = authority revoked.
    pub mint_authority: Option<String>,
    /// `None` = authority revoked.
    pub freeze_authority: Option<String>,
}

#[derive(Debug, Error)]
pub enum MintParseError {
    #[error("mint account data is {0} bytes; expected 82 (SPL token layout)")]
    BadLength(usize),
    #[error("mint account is not initialized")]
    NotInitialized,
}

/// Decode an 82-byte SPL token mint. Hand-rolled (no spl-token dep) — the
/// layout is part of the teaching value, and one less dependency is one
/// less supply-chain surface.
pub fn parse_mint_account(data: &[u8]) -> Result<MintAccount, MintParseError> {
    if data.len() != 82 {
        return Err(MintParseError::BadLength(data.len()));
    }
    if data[45] == 0 {
        return Err(MintParseError::NotInitialized);
    }
    let authority = |tag: u8, key: &[u8]| {
        if tag == 0 {
            None
        } else {
            // SPL pubkeys are displayed base58, not base64.
            Some(base58_encode(key))
        }
    };
    Ok(MintAccount {
        supply_raw: u64::from_le_bytes(
            data[36..44]
                .try_into()
                .map_err(|_| MintParseError::BadLength(data.len()))?,
        ),
        decimals: data[44],
        mint_authority: authority(data[0], &data[4..36]),
        freeze_authority: authority(data[46], &data[47..79]),
    })
}

fn base58_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits = vec![0u8; bytes.len() * 138 / 100 + 1];
    let mut length = 0usize;
    for &byte in bytes {
        let mut carry = u32::from(byte);
        let mut index = 0;
        // Run the carry through the active digits; a byte that fits without
        // overflow still lands in digits[0], so `index` (not an overflow
        // counter) is what advances the significant length.
        while carry > 0 || index < length {
            carry += u32::from(digits[index]) << 8;
            digits[index] = (carry % 58) as u8;
            carry /= 58;
            index += 1;
        }
        length = index;
    }
    let mut zeros = 0usize;
    while zeros < bytes.len() && bytes[zeros] == 0 {
        zeros += 1;
    }
    let mut encoded = String::with_capacity(zeros + length);
    for _ in 0..zeros {
        encoded.push('1');
    }
    for digit in digits[..length].iter().rev() {
        encoded.push(ALPHABET[*digit as usize] as char);
    }
    encoded
}

/// A token account entry from `getTokenLargestAccounts`.
#[derive(Debug, Clone, Deserialize)]
pub struct HolderAmount {
    pub address: String,
    pub amount_raw: u64,
}

/// The seam for swapping transports (public websocket now, Helius later).
///
/// Read methods open no subscriptions: they send one request and wait for
/// the matching response, skipping anything else on the socket.
#[async_trait]
pub trait ChainProvider: Send {
    /// Next matching log notification, or an error (caller decides whether to
    /// reconnect based on it).
    async fn next_event(&mut self) -> Result<LogNotification, ProviderError>;

    /// (Re)subscribe to program-log mentions. Called after every connect.
    async fn subscribe_program_logs(
        &mut self,
        programs: &[&str],
        commitment: &str,
    ) -> Result<(), ProviderError>;

    /// Mint account decoded from raw data. `Ok(None)` = account missing.
    async fn get_mint(&mut self, mint: &str) -> Result<Option<MintAccount>, ProviderError>;

    /// Largest token accounts for a mint (may be fewer than 20).
    async fn get_holders(&mut self, mint: &str) -> Result<Vec<HolderAmount>, ProviderError>;

    /// Fully parsed transaction (encoding `jsonParsed`), `None` if pruned.
    async fn get_parsed_transaction(
        &mut self,
        signature: &str,
    ) -> Result<Option<serde_json::Value>, ProviderError>;
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Websocket JSON-RPC provider for a standard Solana RPC endpoint.
///
/// Streaming (`logsSubscribe`/`next_event`) rides the websocket; the
/// one-shot read methods go over HTTP, because standard Solana websocket
/// endpoints do not serve generic JSON-RPC methods at all.
pub struct WsProvider {
    ws_url: String,
    http_url: String,
    http: reqwest::Client,
    stream: WsStream,
    /// subscription id (== request id we used) -> program id
    subscriptions: HashMap<u64, String>,
    next_id: u64,
}

impl WsProvider {
    pub async fn connect(ws_url: &str) -> Result<Self, ProviderError> {
        // rustls refuses to auto-pick a crypto provider when the crate
        // graph leaves the choice ambiguous; pin `ring` explicitly.
        // install_default is process-global and fails harmlessly if another
        // component already installed one.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let (stream, _) = connect_async(ws_url).await?;
        let http_url = ws_url
            .replacen("wss://", "https://", 1)
            .replacen("ws://", "http://", 1);
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ProviderError::Protocol(format!("http client: {e}")))?;
        Ok(Self {
            ws_url: ws_url.to_string(),
            http_url,
            http,
            stream,
            subscriptions: HashMap::new(),
            next_id: 1,
        })
    }

    pub fn url(&self) -> &str {
        &self.ws_url
    }

    /// Send one whitelisted read-only request over HTTP and return the
    /// `result` field. HTTP keeps no connection state, so this is safe to
    /// call concurrently with the streaming loop on the same provider.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, ProviderError> {
        let request = RpcRequest::new(self.next_id, method, params)?;
        self.next_id += 1;
        let response: Value = self
            .http
            .post(&self.http_url)
            .json(&request.to_json())
            .send()
            .await
            .map_err(|e| ProviderError::Protocol(format!("{method}: http send: {e}")))?
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(format!("{method}: http decode: {e}")))?;
        if let Some(error) = response.get("error") {
            return Err(ProviderError::Rpc(format!("{method}: {error}")));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn send(&mut self, request: &RpcRequest) -> Result<(), ProviderError> {
        let text = serde_json::to_string(&request.to_json())
            .map_err(|e| ProviderError::Protocol(format!("serialize request: {e}")))?;
        self.stream.send(Message::Text(text)).await?;
        Ok(())
    }
}

#[async_trait]
impl ChainProvider for WsProvider {
    async fn subscribe_program_logs(
        &mut self,
        programs: &[&str],
        commitment: &str,
    ) -> Result<(), ProviderError> {
        // The server assigns each subscription its own id, returned as the
        // `result` of our subscribe request — NOT our request id. Wait for
        // every confirmation and map server subscription id -> program.
        let mut pending: HashMap<u64, String> = HashMap::new();
        for program in programs {
            let id = self.next_id;
            self.next_id += 1;
            let request = RpcRequest::new(
                id,
                "logsSubscribe",
                json!([{ "mentions": [program] }, { "commitment": commitment }]),
            )?;
            self.send(&request).await?;
            pending.insert(id, (*program).to_string());
        }

        let mut mapped: HashMap<u64, String> = HashMap::new();
        while !pending.is_empty() {
            let frame =
                tokio::time::timeout(std::time::Duration::from_secs(30), self.stream.next())
                    .await
                    .map_err(|_| {
                        ProviderError::Protocol(
                            "timed out waiting for subscription confirmation".into(),
                        )
                    })?;
            match frame {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|e| ProviderError::Protocol(format!("bad json: {e}")))?;
                    if let Some(request_id) = value.get("id").and_then(Value::as_u64) {
                        let Some(program) = pending.remove(&request_id) else {
                            continue;
                        };
                        if let Some(error) = value.get("error") {
                            return Err(ProviderError::Rpc(format!("{program}: {error}")));
                        }
                        let subscription_id =
                            value.get("result").and_then(Value::as_u64).ok_or_else(|| {
                                ProviderError::Protocol(
                                    "subscription confirmation without numeric result".into(),
                                )
                            })?;
                        mapped.insert(subscription_id, program);
                    }
                    // Notifications can interleave before the final
                    // confirmation; they are safe to skip here.
                }
                Some(Ok(Message::Ping(_))) => {
                    self.stream.send(Message::Pong(Vec::new())).await?;
                }
                Some(Err(error)) => return Err(error.into()),
                _ => {
                    return Err(ProviderError::Protocol(
                        "stream ended during subscribe".into(),
                    ))
                }
            }
        }
        self.subscriptions = mapped;
        Ok(())
    }

    async fn next_event(&mut self) -> Result<LogNotification, ProviderError> {
        loop {
            match self.stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|e| ProviderError::Protocol(format!("bad json: {e}")))?;

                    if let Some(subscription) = value
                        .pointer("/params/subscription")
                        .and_then(Value::as_u64)
                    {
                        let signature = value
                            .pointer("/params/result/value/signature")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                ProviderError::Protocol("notification without signature".into())
                            })?;
                        let slot = value
                            .pointer("/params/result/context/slot")
                            .and_then(Value::as_u64)
                            .ok_or_else(|| {
                                ProviderError::Protocol("notification without slot".into())
                            })?;
                        let logs: Vec<String> = value
                            .pointer("/params/result/value/logs")
                            .and_then(Value::as_array)
                            .map(|entries| {
                                entries
                                    .iter()
                                    .filter_map(|entry| entry.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let program = self
                            .subscriptions
                            .get(&subscription)
                            .cloned()
                            .unwrap_or_else(|| format!("<unmapped sub={subscription}>"));
                        return Ok(LogNotification {
                            signature: signature.to_string(),
                            slot,
                            program,
                            logs,
                            subscription_id: subscription,
                        });
                    }

                    if let Some(error) = value.get("error") {
                        return Err(ProviderError::Rpc(error.to_string()));
                    }
                    // Subscription confirmations and stray messages: ignore.
                }
                Some(Ok(Message::Ping(_))) => {
                    self.stream.send(Message::Pong(Vec::new())).await?;
                }
                Some(Ok(Message::Close(frame))) => {
                    return Err(ProviderError::Protocol(format!(
                        "endpoint closed the socket: {frame:?}"
                    )));
                }
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(_)) => {}
                None => return Err(ProviderError::Protocol("stream ended".into())),
            }
        }
    }

    async fn get_mint(&mut self, mint: &str) -> Result<Option<MintAccount>, ProviderError> {
        use base64::Engine;
        let result = self
            .call("getAccountInfo", json!([mint, { "encoding": "base64" }]))
            .await?;
        let Some(value) = result.get("value") else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let data = value
            .get("data")
            .and_then(Value::as_array)
            .and_then(|pair| pair.first())
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::Protocol("getAccountInfo without data".into()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ProviderError::Protocol(format!("base64 mint data: {e}")))?;
        parse_mint_account(&bytes)
            .map(Some)
            .map_err(|e| ProviderError::Protocol(e.to_string()))
    }

    async fn get_holders(&mut self, mint: &str) -> Result<Vec<HolderAmount>, ProviderError> {
        let result = self
            .call(
                "getTokenLargestAccounts",
                json!([mint, { "commitment": "confirmed" }]),
            )
            .await?;
        let value = result
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let holders = value
            .iter()
            .filter_map(|entry| {
                let address = entry.get("address").and_then(Value::as_str)?.to_string();
                let amount_raw: u64 = entry.get("amount").and_then(Value::as_str)?.parse().ok()?;
                Some(HolderAmount {
                    address,
                    amount_raw,
                })
            })
            .collect();
        Ok(holders)
    }

    async fn get_parsed_transaction(
        &mut self,
        signature: &str,
    ) -> Result<Option<serde_json::Value>, ProviderError> {
        let result = self
            .call(
                "getTransaction",
                json!([
                    signature,
                    {
                        "encoding": "jsonParsed",
                        "commitment": "confirmed",
                        // launchpad txs are versioned; without this the
                        // endpoint refuses them outright (-32015)
                        "maxSupportedTransactionVersion": 1
                    }
                ]),
            )
            .await?;
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base58_matches_reference_vectors() {
        // Vectors cross-checked against the reference base58 implementation.
        assert_eq!(base58_encode(&[]), "");
        assert_eq!(base58_encode(&[0x00]), "1");
        assert_eq!(base58_encode(&[0x00, 0x00]), "11");
        assert_eq!(base58_encode(&[0x01]), "2");
        assert_eq!(base58_encode(&[0x3d]), "24"); // 61 = 1*58 + 3
        let seq: Vec<u8> = (0..32).collect();
        assert_eq!(
            base58_encode(&seq),
            "1thX6LZfHDZZKUs92febYZhYRcXddmzfzF2NvTkPNE"
        );
        assert_eq!(
            base58_encode(&[0xff; 32]),
            "JEKNVnkbo3jma5nREBBJCDoXFVeKkD56V3xKrvRmWxFG"
        );
    }

    #[test]
    fn parse_mint_decodes_authority_base58() {
        // Real layout: supply 1_000_000 at [36..44], decimals 6, mint
        // authority COption(1) + a key of all-0x01 bytes, freeze revoked.
        let mut data = [0u8; 82];
        data[0] = 1;
        data[4..36].fill(0x01);
        data[36..44].copy_from_slice(&1_000_000u64.to_le_bytes());
        data[44] = 6;
        data[45] = 1; // initialized
        let mint = parse_mint_account(&data).expect("parses");
        assert_eq!(
            mint.mint_authority.as_deref(),
            Some("4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi")
        );
        assert_eq!(mint.freeze_authority, None);
        assert_eq!(mint.supply_raw, 1_000_000);
        assert_eq!(mint.decimals, 6);
    }

    #[test]
    fn whitelist_rejects_everything_not_readonly() {
        assert!(RpcRequest::new(1, "sendTransaction", json!([])).is_err());
        assert!(RpcRequest::new(1, "simulateTransaction", json!([])).is_err());
        assert!(RpcRequest::new(1, "signAndSendTransaction", json!([])).is_err());
        assert!(RpcRequest::new(1, "requestAirdrop", json!([])).is_err());
        assert!(RpcRequest::new(1, "getBalance", json!([])).is_err()); // not needed either
    }

    #[test]
    fn whitelist_accepts_readonly_methods() {
        for method in ALLOWED_METHODS {
            assert!(
                RpcRequest::new(1, method, json!([])).is_ok(),
                "{method} should be allowed"
            );
        }
    }
}
