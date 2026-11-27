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

| Module | Responsibility |
|--------------------|----------------|
| `config` | Load `config.toml` (gitignored); fall back to built-in defaults. *(M1, shipped)* |
| `provider` | The only RPC boundary. `ChainProvider` trait hides the transport; implemented over websocket JSON-RPC with a **read-only method whitelist** (enforced at request construction, guarded by tests) and server-confirmed subscription mapping. When `rpc.api_key` is set, one-shot reads route through `mainnet.helius-rpc.com` under a shared metered budget (`HeliusMeter`: per-day cap, rollover, automatic per-call fallback to the public endpoint); streaming stays on the public websocket. *(M2, shipped; metered reads shipped with the Helius phase)* |
| `listener` | `logsSubscribe` mentions on Raydium AMM v4 / Orca Whirlpools / Pump.fun / PumpSwap; filters raw mentions (which fire on every swap) down to pool-creation instructions, excluding reverted transactions; reconnect with exponential backoff + jitter; plain-language status for the UI. *(M2, shipped)* |
| `pipeline` | Bounded queue (backpressure: full submissions counted and surfaced as degraded, never blocking the listener); per-signature dedupe with an 8 192-entry ring; drains into the store. *(M2, shipped)* |
| `store` | Append-only JSONL persistence with CRC32-envelope integrity; refuses to open on checksum mismatch. *(M2, shipped)* |
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
code; the file is gitignored. The provider reads `rpc.ws_url` (streaming +
public read fallback) and, when present, `rpc.api_key` + `rpc.daily_credit_cap`
for the metered Helius read path — the swap is a config-level change.

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
