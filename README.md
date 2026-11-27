# TrenchWatch

<img src="assets/hero.png" alt="TrenchWatch banner: radar sweep over Solana launches" width="100%">

Real-time monitor for brand-new token launches on Solana mainnet-beta, with
explainable rug-pull risk scoring. Every flag ships with plain-language
evidence and an on-chain receipt you can verify yourself — no black-box
scores.

Built to demonstrate hands-on Solana data-infrastructure skills: websocket
ingestion under rate limits, account parsing, and evidence-backed heuristics.

## What it does

- **Watches** the pump.fun launch feed over a websocket RPC subscription,
  in real time, and follows each token's full lifecycle on Solana
  mainnet-beta.
- **Scores** each launch against six rug-pull signals (mint/freeze authority,
  LP burn/lock, dev-wallet funding, dev-wallet history, holder concentration,
  thin liquidity). Each signal produces `{flag, severity, explanation,
  on-chain evidence, field-guide reference}` — see
  [docs/FIELD_GUIDE.md](docs/FIELD_GUIDE.md).
- **Detects graduations**: when a launch's liquidity migrates from the
  pump.fun bonding curve to a PumpSwap pool, TrenchWatch links the migration
  to the original launch by token mint — the birth verdict and baggage
  follow the token onto the DEX. Tokens created directly on a DEX pool
  (increasingly the norm on PumpSwap AMM) are flagged "born on DEX" instead —
  no curve, no graduation, and the pool-creation tx is the birth receipt.
- **Shows** a dense mission-control dashboard: live table, detail drawer with
  full evidence receipts, verdict/lifecycle filters, a "New on DEXs" view
  for freshly graduated tokens.
- **Alerts** via desktop notification and optional Discord webhooks — flag
  alerts and graduation alerts can post to separate channels.

## See it in action

Running live against Solana mainnet-beta — real launches, real verdicts:

<img src="assets/screenshots/dashboard.png" alt="Live dashboard: verdict filter, token mints, launchpad, slot, and per-launch signal counts" width="100%">

Every row opens a full evidence receipt — identity, signature, the exact
risk flags with their on-chain proof, and an honest list of what hasn't
been assessed yet:

<img src="assets/screenshots/evidence-drawer.png" alt="Launch analysis drawer: critical flags with plain-language explanations, observed slot receipts, and field-guide references" width="100%">

## Watch it fire live

A community TrenchWatch watcher posts real graduations and critical-flag
receipts as they happen — no setup required:

**[Join the TrenchWatch alerts server](https://discord.gg/SNjAp6FQB)**
(the same invite is one click away in the app's top bar).

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
- `ChainProvider` trait isolates RPC behind one interface. The launch
  stream rides the free public websocket; set `rpc.api_key` and the
  analysis reads (parsed transactions, mint accounts, holders) switch to
  metered Helius calls with a configurable daily cap and automatic
  fallback to the public endpoint — a config-level change, zero refactor.
  Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- React + TypeScript frontend, no telemetry, CSP-locked.

## Quickstart (~5 minutes on a fresh Windows machine)

No accounts, no API keys, no cost: TrenchWatch runs entirely on Solana's
free public RPC out of the box — everything below works as-is. The optional
[Helius key](#optional-helius-enrichment) and
[Discord webhooks](#optional-discord-alerts) are upgrades you can paste in
later, not requirements.

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

The first `tauri dev` builds the Rust backend from scratch — expect a few
minutes of compiler output before the window opens; later starts are
seconds. On first launch a short setup wizard walks you through the two
optional paste-ins — a Helius API key and a Discord webhook — with a skip
path for both. Either way, the app connects to mainnet-beta and starts
surfacing launches within a minute. It runs entirely on your machine; there
is no server to host and nothing to deploy. Everything the wizard collects
lands in `config.toml` next to the app data (gitignored) and stays editable
in the Settings drawer — see the [config reference](#configuration) below.

## Configuration

All settings live in `config.toml` (gitignored — never commit it). Every
key is documented in [config.example.toml](config.example.toml):

| Key | Default | Meaning |
|-----|---------|---------|
| `rpc.ws_url` | public mainnet-beta endpoint | WebSocket RPC URL. The launch stream always rides this. |
| `rpc.api_key` | `""` | Optional Helius API key. When set, analysis reads switch to metered Helius calls; empty = public endpoint for everything. |
| `rpc.daily_credit_cap` | `100000` | Daily budget for Helius read calls (`0` = unlimited, not recommended). At the cap the app falls back to the public endpoint. |
| `monitor.min_liquidity_usd` | `0` | Liquidity floor for surfacing launches. |
| `alerts.desktop_notifications` | `true` | Desktop notification on flagged launches at or above `alerts.min_severity`. |
| `alerts.min_severity` | `"critical"` | Lowest severity that alerts: `critical`, `warn`, or `info`. |
| `alerts.discord_webhook_url` | `""` | Optional Discord webhook; empty disables. Graduation alerts always post here. |
| `alerts.discord_critical_webhook_url` | `""` | Optional second webhook for flag alerts (e.g. a `#critical-flags` channel). Empty = flag alerts use `discord_webhook_url` too. |
| `alerts.graduation_alerts` | `true` | Announce graduations (launchpad → DEX pool) even when no risk flag fired. |
| `storage.store_dir` | `"store"` | Local event store location. |

### Optional: Helius enrichment

Skip this section and everything still works — this is purely an upgrade
for faster, more reliable analysis reads, and it only spends credits on
those reads (never on the launch stream).

The first-run wizard asks for the key, and the easiest path is to paste it
there — it applies immediately, no restart. Prefer files? Set `api_key`
under `[rpc]` in `config.toml` next to the app data directory (already
gitignored — never the repo).

1. The top bar shows `helius: used / cap` counting read calls for the day;
   the launch stream stays on the free websocket either way.
2. If Helius errors or the cap is hit, reads fall back to the public
   endpoint automatically — analysis never stalls.

### Optional: Discord alerts

Get flagged-launch receipts and graduation announcements posted to Discord
channels you control. Skip this section and desktop notifications still
cover you — Discord is just for alerts that follow you onto your phone.

The first-run wizard walks you through the main webhook. To set up or
change webhooks later, open the Settings drawer. Everything below is the
same recipe, spelled out.

**Setting it up (about 2 minutes):**

1. Open Discord and pick a server where you can manage webhooks — any
   server you own works. Don't have one? Click the `+` in the server
   list and create a free private server just for alerts.
2. Right-click the server name → **Server Settings** → **Integrations** →
   **Webhooks** → **New Webhook**.
3. Pick the channel the alerts should land in (for example `#alerts`),
   give the webhook any name — the app overrides it with `TrenchWatch` —
   then click **Copy Webhook URL**. It looks like
   `https://discord.com/api/webhooks/<id>/<token>`.
4. Paste that URL into the **discord webhook** field in the app's
   Settings drawer (or `alerts.discord_webhook_url` in `config.toml`
   next to the app data directory). Saving takes effect immediately.
5. Want flag alerts in a separate channel (like `#critical-flags`)?
   Create a second webhook there and paste it into the
   **critical-flags webhook** field. Graduations always post to the main
   webhook, so the two feeds stay cleanly separated.
6. Choose how chatty it should be: the **alert threshold** in Settings
   defaults to critical-only. Set it to `warn` or `every flag` for
   more traffic.

**What you'll receive:** one message per flagged launch, styled as an
embed with the severity color, the token's mint address, the on-chain
signature, and the top risk flag in plain words — and one message per
graduation, including the birth verdict ("flagged at birth: N critical")
so the baggage follows the token onto the DEX.

**What leaves your machine:** nothing but those POSTs to the exact URL
you pasted. No analytics, no telemetry. If Discord rejects or can't be
reached, the alert is skipped and the reason is logged — monitoring
never stalls on a chat outage.

## Docs

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — module map and data flow.
- [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) — adversaries, trust
  boundaries, and how non-goals are structurally enforced.
- [docs/FIELD_GUIDE.md](docs/FIELD_GUIDE.md) — every signal explained in
  plain language, with false-positive rates.

## Contributing

Contributions welcome — open an issue or PR and it will be handled case by
case. Conventional commits, clippy-clean Rust, every risk signal must ship
with evidence and tests.

## Disclaimer

TrenchWatch produces heuristics with evidence, not financial advice. It
never labels a token "safe" — only "no flags found as of slot N". Public
chain data can be spoofed (see the threat model); do your own research.

## License

[MIT](LICENSE)
