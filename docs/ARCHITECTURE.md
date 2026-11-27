# TrenchWatch Architecture

> Draft v1 (M1). Updated as milestones land. Read together with
> `docs/THREAT_MODEL.md` — security decisions are documented there, not here.

## 1. Overview

TrenchWatch is a Tauri v2 desktop app: a Rust backend owns all chain I/O,
risk analysis, storage, and alerting; a React frontend renders a read-only
mission-control dashboard over Tauri IPC. The backend is the only
component that touches the network (plus the optional Discord webhook).

```
                        ┌──────────────────────────────┐
                        │        React frontend       │
                        │  live table · drawer · filters│
                        └──────────────┬───────────────┘
                                       │ IPC (commands, read-only)
┌──────────────┐   wss    ┌────────────▼─────────────┐   ┌──────────────┐
│ Solana       │─────────▶│  Rust backend            │──▶│ Local store  │
│ mainnet-beta │          │                          │   │ (append)   │
└──────────────┘          │  provider → pipeline →   │   └──────────────┘
                          │  risk engine → alerts    │
                          └────────────┬─────────────┘
                                       │ optional
                               ┌───────▼────────┐
                               │ Discord webhook │
                               └────────────────┘
```

## 2. Backend modules (`src-tauri/src/`)

| Module (planned) | Responsibility |
|--------------------|----------------|
| `config` | Load `config.toml` (gitignored); fall back to built-in defaults. *(M1)* |
| `provider` | The only RPC boundary. A `ChainProvider` trait hides the transport; v1 implements it over websocket JSON-RPC with a **read-only method whitelist**, reconnect with exponential backoff + jitter, and request coalescing. A Helius-backed implementation (API key + webhooks) must drop in with zero changes to callers. *(M2)* |
| `listener` | `logsSubscribe` subscriptions filtered to Raydium / Orca / PumpSwap pool-creation programs + Pump.fun graduation; normalizes every hit into a `LaunchEvent`. *(M2)* |
| `pipeline` | Bounded queue with backpressure; dedupes per mint; fans out to the risk engine; marks listener degraded on saturation instead of dropping new events. *(M2)* |
| `store` | Append-oriented local persistence of events + evidence receipts; checksum on load. *(M2)* |
| `risk` | Pure functions: `(fetched state, launch context) → Vec<RiskFlag>`. No network, no ambient clock — reproducible from receipts. *(M3)* |
| `evidence` | Receipt type: `{flag, severity, explanation, evidence, field_guide_ref, slot, observed_at}`. Serialized into the store and shipped to UI + alerts verbatim. *(M3)* |
| `alerts` | Desktop notifications + Discord webhook, receipts-style, escaped. *(M5)* |
| `commands` | Tauri IPC surface: query events/flags, control listener, apply filters. Read-only queries plus start/stop; nothing that could mutate chain state. *(M2+)* |

## 3. Risk engine contract

Every signal outputs a structured receipt — never a bare score:

```rust
struct RiskFlag {
    signal: SignalKind,        // e.g. MintAuthorityActive
    severity: Severity,        // Info | Warn | Critical
    explanation: String,       // plain language, one or two sentences
    evidence: Evidence,        // on-chain data the conclusion rests on
    field_guide_ref: String,   // anchor into docs/FIELD_GUIDE.md
    slot: u64,                 // when it was true
    observed_at: DateTime<Utc>,
}
```

Aggregated verdicts are derived (max severity + flag count), never stored
without their receipts. If you can't show the receipt, the flag doesn't ship.

## 4. Frontend (`src/`)

React + TypeScript + Vite, no UI framework beyond hand-rolled CSS
(mission-control theme tokens in `src/styles/global.css`). M4 builds:

- live table, newest first: token, age, LP status, flags, dev wallet link;
- detail drawer with full evidence receipts per flag;
- filters: min liquidity, hide-flagged toggle, chain selector placeholder;
- everything token-derived rendered as inert text (threat model §3.A1).

## 5. Configuration

Single `config.toml` (see `config.example.toml` for every key). No keys in
code; the file is gitignored. The provider reads only `rpc.ws_url`, keeping
the Helius swap a config-level change.

## 6. Testing strategy

- `risk` is pure → unit tests + fixtures from real (sanitized) on-chain
  examples; each fixture is a JSON bundle of account data + expected flags.
- Provider interface faked in tests; integration tests hit mainnet-beta
  sparingly and are marked `#[ignore]` by default (public endpoint etiquette).
- Frontend: type checking (`tsc`) + component smoke tests where behavior is
  non-trivial.

## 7. Build & run

```bash
npm install
npm run tauri dev     # canonical command: frontend + backend + window
```

Requires Node.js, the Rust toolchain, and (Windows) the WebView2 runtime.
