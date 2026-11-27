# TrenchWatch

Real-time monitor for brand-new token launches on Solana mainnet-beta, with
explainable rug-pull risk scoring. Every flag ships with plain-language
evidence and an on-chain receipt you can verify yourself — no black-box
scores.

Built to demonstrate hands-on Solana data-infrastructure skills: websocket
ingestion under rate limits, account parsing, and evidence-backed heuristics.
(DevRel portfolio piece.)

![screenshot](docs/assets/screenshot-placeholder.png)

## What it does

- **Watches** Raydium, Orca, and PumpSwap pool creations + Pump.fun
  graduations over a websocket RPC subscription, in real time.
- **Scores** each launch against six rug-pull signals (mint/freeze authority,
  LP burn/lock, dev-wallet funding, dev-wallet history, holder concentration,
  thin liquidity). Each signal produces `{flag, severity, explanation,
  on-chain evidence, field-guide reference}` — see
  [docs/FIELD_GUIDE.md](docs/FIELD_GUIDE.md).
- **Shows** a dense mission-control dashboard: live table, detail drawer with
  full evidence receipts, filters.
- **Alerts** via desktop notification and optional Discord webhook.

## What it deliberately cannot do

TrenchWatch is **read-only by design**: no trading, no private keys, no
wallet custody, no transaction signing. There is no code path that could move
funds — the RPC layer exposes only a whitelist of read-only methods. The
full reasoning is in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md), written
before any functional code.

## Architecture

```
[ Solana mainnet-beta ]──wss──>[ provider ]─>[ pipeline ]─>[ risk engine ]
                                                              │
                              [ dashboard ]◀──IPC──[ store ]──┤
                                                              ▼
                                                     [ alerts: desktop + Discord ]
```

- Rust backend (Tauri v2) owns all chain I/O, analysis, storage, alerting.
- `ChainProvider` trait isolates RPC behind one interface — swapping the
  free public endpoint for a Helius API key + webhooks later is a
  config-level change, zero refactor. Details:
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- React + TypeScript frontend, no telemetry, CSP-locked.

## Quickstart (~10 minutes on a fresh Windows machine)

Prerequisites: [Node.js](https://nodejs.org) (LTS), the
[Rust toolchain](https://rustup.rs) (`rustup-init.exe` → default install,
includes MSVC build tools via Visual Studio Build Tools), and WebView2
(pre-installed on Windows 11).

```bash
git clone <repo-url> trenchwatch
cd trenchwatch
npm install
npm run tauri dev
```

First run uses built-in defaults (public endpoint, no Discord). To
customize, copy `config.example.toml` to `config.toml` next to the app and
edit — see the [config reference](#configuration) below.

## Configuration

All settings live in `config.toml` (gitignored — never commit it). Every
key is documented in [config.example.toml](config.example.toml):

| Key | Default | Meaning |
|-----|---------|---------|
| `rpc.ws_url` | public mainnet-beta endpoint | WebSocket RPC URL. A dedicated provider (e.g. Helius) improves reliability. |
| `monitor.desktop_notifications` | `true` | Notify on each launch passing filters. |
| `monitor.min_liquidity_usd` | `0` | Liquidity floor for surfacing launches. |
| `alerts.discord_webhook_url` | `""` | Optional Discord webhook; empty disables. |
| `storage.store_dir` | `"store"` | Local event store location. |

## Docs

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — module map and data flow.
- [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) — adversaries, trust
  boundaries, and how non-goals are structurally enforced.
- [docs/FIELD_GUIDE.md](docs/FIELD_GUIDE.md) — every signal explained in
  plain language, with false-positive rates.

## Contributing

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Conventional
commits, clippy-clean Rust, every risk signal must ship with evidence and
tests.

## Disclaimer

TrenchWatch produces heuristics with evidence, not financial advice. It
never labels a token "safe" — only "no flags found as of slot N". Public
chain data can be spoofed (see the threat model); do your own research.

## License

[MIT](LICENSE)
