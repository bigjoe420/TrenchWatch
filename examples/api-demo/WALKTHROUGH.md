# Helius API demo — walkthrough

Minimal Vite 5 + TypeScript (strict, no framework) showing three real
Helius calls against Solana mainnet-beta. This is the same API surface
TrenchWatch's backend uses: one standard JSON-RPC call and two DAS
(Digital Asset Standard) reads.

## Run it

```bash
cd examples/api-demo
npm install
cp .env.example .env.local   # then paste your key into .env.local
npm run dev                  # http://localhost:1421
```

The key is read from `VITE_HELIUS_API_KEY` in `.env.local` (gitignored).
Restart `npm run dev` after editing `.env.local` — Vite only loads env
files at startup.

## The five screens

### 1. First load, no key configured
The header shows `✗ no key` and the key-status line explains what to do.
Every card's **run** button still works — it renders the honest config
error instead of a blank page. Failure states are a feature here, not an
embarrassment.

### 2. Key configured — liveness call
Click **run** on card 1 (`getLatestBlockhash`). Success renders green:
the slot (with thousands separators) and the blockhash. This proves the
key, the endpoint, and the chain in one round trip.

### 3. DAS signatures for the pump.fun program
Card 2 (`getSignaturesForAsset` on the pump.fun program id) lists the five
most recent signatures: slot, truncated signature, and a `(failed tx)`
marker when the transaction errored. This is the "what's happening on the
launchpad right now" read.

### 4. DAS asset record for a real mint
Card 3 (`getAsset`) fetches the full Digital Asset Standard record for a
real pump.fun mint that TrenchWatch observed on mainnet-beta: name, symbol,
interface, and standard.

### 5. Error states, on demand
- Delete the key from `.env.local` and restart → every card shows the
  `[config]` error (screen 1 again).
- Unplug the network and click **run** → `[network]` or `[timeout]`.
- Feed the page a revoked key → `[http] 403` from Helius.
- Ask for a nonsense mint → `[rpc]` JSON-RPC error with Helius's message.

## Honesty notes

- Fetch only — no SDK hides what's on the wire.
- One 10-second timeout per call, surfaced as `[timeout]`, no silent retry.
- The key never appears in source, in the DOM, or in any committed file.
