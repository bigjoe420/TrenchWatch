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
use serde::Serialize;
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

/// The seam for swapping transports (public websocket now, Helius later).
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
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Websocket JSON-RPC provider for a standard Solana RPC endpoint.
pub struct WsProvider {
    ws_url: String,
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
        Ok(Self {
            ws_url: ws_url.to_string(),
            stream,
            subscriptions: HashMap::new(),
            next_id: 1,
        })
    }

    pub fn url(&self) -> &str {
        &self.ws_url
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
