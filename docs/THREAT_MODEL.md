# TrenchWatch Threat Model

> Status: v1, written **before** functional code (milestone M1).
> Every mitigation listed here is either structural (the code cannot do
> otherwise) or enforced by review. Where a risk is accepted, the rationale
> is stated.

TrenchWatch is a **read-only** observer of Solana mainnet-beta. It watches
for new token launches and scores them for rug-pull risk. It never holds
keys, never signs, never transacts. That single fact eliminates the largest
class of crypto-application threats — but it creates others, and this
document enumerates all of them.

---

## 1. Assets

| Asset | Why it matters |
|-------|----------------|
| The operator's machine | The app runs locally with the operator's network access. A compromised TrenchWatch is a beachhead. |
| RPC endpoint availability & quota | Free public endpoints rate-limit aggressively; a naive client can get its IP throttled or banned. |
| Integrity of displayed assessments | A risk score someone *trusts* and acts on is only as good as the evidence behind it. Fabricated or stale evidence is worse than no evidence. |
| Reputation of the project | TrenchWatch's output is public-facing (alerts, screenshots). Wrong claims about real tokens invite justified criticism. |
| Local event store | Historical launch data accumulated over time; tampering would distort analysis. |

## 2. Non-goals — enforced structurally, not by policy

These capabilities **must not exist anywhere in the codebase or its
dependency tree**. "Detectable by audit" is not sufficient; where possible
the architecture makes the capability impossible.

| Non-goal | Structural enforcement |
|----------|----------------------|
| No trading / swapping | There is no transaction-building code. The RPC layer exposes **only a whitelist of read-only methods** (`logsSubscribe`, `logsUnsubscribe`, `getAccountInfo`, `getTokenSupply`, `getTokenAccountBalance`, `getSignaturesForAddress`, `getTransaction`, `getTokenLargestAccounts`, `getSlot`, `getProgramAccounts`). `sendTransaction`, `simulateTransaction` with signers, and any `sign*` method are not in the whitelist and there is no generic passthrough. |
| No private-key handling / custody | No keypair, seed-phrase, or signing crate may appear in `Cargo.toml`/`package.json`. No type representing secret key material exists. A private key pasted anywhere in the app has nothing to attach to. |
| No transaction signing | Nothing in the dependency tree imports a signing API. The CSP (see below) additionally blocks dynamic script execution that could smuggle one in. |

Review gate: every dependency addition is checked against this table, and a
unit test enumerates the RPC method whitelist so a regression fails loudly.

## 3. Adversaries and capabilities

### A1. Token deployers (the monitored population)
The tokens TrenchWatch analyzes are *adversarial input by construction*.
Deployers benefit from being scored as low-risk.

- **Malicious metadata.** Token names/symbols/URIs can contain anything:
  XSS payloads, unicode tricks, phishing URLs, impersonation of known tokens.
  **Mitigation:** all token strings are treated as untrusted data — rendered
  as inert text (React escaping, no `dangerouslySetInnerHTML`, CSP without
  `unsafe-eval`), length-capped, and displayed URLs are never auto-fetched.
- **Timing games.** A deployer can launch with clean authorities, then
  mutate state after the scan. **Mitigation:** every flag records the slot it
  was observed at; the UI shows scan age; periodic re-scans refresh state
  and the evidence trail keeps both snapshots. Flags are "as of slot N",
  never absolute claims.
- **Noise / spam.** Launch spam can flood the UI and exhaust RPC quota.
  **Mitigation:** per-mint de-duplication, configurable liquidity floor,
  and bounded queueing with backpressure (dropping oldest pending scans
  under saturation, never the newest).
- **Sybil reputation farming.** Deployers spread activity across fresh
  wallets to defeat history-based signals. **Accepted risk**, documented in
  the field guide with its measured false-positive/false-negative behavior.

### A2. Malicious or compromised RPC endpoint
The operator configures the endpoint; it could be malicious, or the free
public endpoint could be intercepted (TLS mitigates but endpoints can lie).

- **Lying about account state.** The endpoint could hide a freeze
  authority or fake a burn. **Mitigation:** evidence receipts record the
  raw account data (or its hash) and the slot/confirmation status it came
  from, so a claim is checkable against any other endpoint. The field guide
  teaches users to verify. Cross-endpoint verification is a later-phase
  feature, listed as accepted risk for v1.
- **Denial of service / rate limits.** **Mitigation:** exponential backoff
  with jitter, single shared connection manager, graceful degradation to
  "listener degraded" status instead of crash loops that hammer the endpoint.
- **Fingerprinting.** Whatever endpoint receives requests learns the
  operator watches specific launches. **Accepted risk** — this is inherent
  to using any remote RPC; using a paid dedicated endpoint (Helius) is the
  documented mitigation and later-phase work.

### A3. Malicious Discord webhook target
The webhook URL is operator-supplied, so the primary risk is misdirection:

- **Exfiltration via webhook.** Nothing sensitive is exfiltratable — the app
  holds no keys and no personal data; alert payloads contain only public
  chain data. **Mitigation:** outbound HTTP is restricted to the single
  configured webhook URL (no redirect following across origins).
- **Webhook content injection.** Token metadata embedded in an alert could
  carry markdown/mention tricks into a Discord channel. **Mitigation:**
  payloads are formatted server-side with escaping; raw token strings are
  truncated and sanitized before inclusion.

### A4. Supply-chain adversary (crates/npm)
A compromised dependency could smuggle key-stealing or network-broadcasting
code into the binary.

- **Mitigation:** minimal, well-audited dependency set; no crypto/signing
  crates (halves the attack surface by construction — a keylogger finds no
  keys); `cargo audit` and `npm audit` in CI; lockfiles committed.

### A5. Social engineering against the operator
An attacker who convinces the operator that TrenchWatch output is a
**guarantee** (rather than an evidence-backed heuristic) can cause real
financial harm without touching the code.

- **Mitigation:** every score ships with plain-language evidence and a
  field-guide reference; the UI and alerts state "heuristics, not financial
  advice"; the field guide documents each signal's false-positive rate.
  TrenchWatch *never* says "safe" — it says "no flags found as of slot N".

### A6. Helius API key handling
The optional `rpc.api_key` unlocks paid read-only enrichment. The key is a
secret the app must spend without ever exposing.

- **Storage.** `config.toml` in the app data directory only — gitignored by
  `.gitignore` from day one; the example file ships an empty value. It never
  enters the repo, the frontend, or the event store.
- **Exposure.** The key travels only inside the HTTPS request URL to
  `mainnet.helius-rpc.com`. It is never written to logs, error strings, or
  `Debug` output (the meter redacts it), and the frontend receives usage
  counts only — never the key, never the endpoint URL.
- **Scope.** Helius receives exactly the same whitelisted read-only methods
  as the public endpoint (`RpcRequest` is enforced before routing), so a
  compromised or misconfigured key cannot turn the app into a signing or
  sending client.
- **Spend control.** Every Helius call is counted against `daily_credit_cap`
  per day; at the cap the app falls back to the public endpoint and keeps
  working, logging the switch once per day. A Helius failure falls back the
  same way per call.
- **Accepted risk.** Helius (like any RPC operator) sees which transactions
  and accounts the operator queries — the fingerprinting tradeoff of §A2,
  now paid instead of free.

## 4. Trust boundaries

```
[ Solana mainnet-beta ]──wss/TLS──>[ provider interface ]
                                          │  (method whitelist,
                                          │   backoff, read-only)
                                          ▼
[ event pipeline ]──>[ risk engine ]──>[ local store ]
                          │                  │
                          ▼                  ▼
              [ evidence receipts ]    [ IPC (Tauri commands) ]
                          │                  │
                          └───────>[ dashboard / alerts ]
```

1. **Network boundary (chain → provider):** untrusted data enters here.
   Parsed strictly, never executed, size-capped.
2. **Engine boundary (pipeline → risk engine):** the engine is pure
   functions over fetched state — no network, no clock access beyond the
   slot context it's handed. This makes every verdict reproducible from its
   receipts.
3. **IPC boundary (backend → frontend):** commands expose read-only query
   and control operations only; frontend cannot reach the network directly
   (CSP `connect-src` whitelist: self + IPC only). The UI renders data from
   the backend, and any external link (Solscan) opens in the system browser,
   never an in-app webview.
4. **Outbound boundary (app → Discord / Helius):** the Discord webhook is
   exactly one operator-configured URL; Helius is contacted only when an API
   key is configured, over the same whitelisted read-only requests (§A6).

## 5. Webview / UI surface

- CSP on both the dev HTML and the Tauri webview: `default-src 'self'`,
  no `unsafe-eval`, no remote origins.
- Token names, URLs, and mint addresses are rendered as plain text.
- The app never embeds a general-purpose browser. External links open via
  the OS shell API with fixed, scheme-checked URLs (https only, known
  explorers only).

## 6. Local store integrity & privacy

- The store holds **public chain data only** — no wallets, no identity, no
  usage telemetry. There is nothing worth stealing in it.
- Writes are append-oriented; corruption is detected at load and the store
  refuses to start with a checksum mismatch rather than serve tampered data.
- The store directory lives in the app-data directory, never in the repo.

## 7. Accepted risks (explicitly out of scope for v1)

| Risk | Why accepted | Planned mitigation |
|------|--------------|--------------------|
| RPC endpoint lies about state | Requires trusting *some* endpoint to be useful at all; receipts make lies checkable | Cross-endpoint verification (later phase) |
| Sybil wallets defeat history signals | Fundamental to public chains | Multi-signal aggregation; field-guide disclosure |
| Public endpoint rate limits | v1 targets the free tier by design | Backoff + provider swap without refactor |
| Alert fatigue | Operator-configurable filters exist in v1 | Smart throttling (later) |

## 8. Review checklist (run before every release)

- [ ] `cargo tree` and `package-lock.json` diff reviewed — no signing/keypair/crypto-wallet crates.
- [ ] RPC method whitelist test passes.
- [ ] No secrets, personal paths, or session files in the repo (see `.gitignore` + CI scan).
- [ ] All token-derived strings render as inert text (grep for `dangerouslySetInnerHTML`, `eval`, raw HTML injection points).
- [ ] Outbound network calls enumerate to: RPC `ws_url`, Discord webhook. Nothing else.
