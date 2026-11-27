# TrenchWatch Field Guide

Every signal TrenchWatch scores, explained in plain language: what it is,
why it matters, exactly how we check it, and where it can be wrong. This is
the document every risk receipt points back to.

**Read this first:** TrenchWatch produces *evidence-backed heuristics*, not
financial advice. "No flags found" never means "safe". It means: with the
data available at that slot, none of the checks below fired. Every verdict
names its slot — on-chain state can change after we look.

False-positive rates below are stated honestly: where we have not yet
measured a rate at scale, the entry says so instead of inventing a number.

---

## S1. Mint / freeze authority

**What it is.** Every SPL token has two optional "master keys": the *mint
authority*, which can print new supply at any time, and the *freeze
authority*, which can freeze any holder's token accounts, blocking them
from transferring (selling).

**Why it matters.** An active mint authority held by a wallet means holders
can be diluted to nothing overnight — the deployer prints a billion tokens
and dumps them. An active freeze authority means the deployer can trap
holders: buys work, sells don't.

**How we check.** We read the mint account's raw 82-byte SPL layout
(`getAccountInfo`) and decode both authorities. Outcomes:

- Both revoked (`None`) → no flag. This is the good case.
- Held by a **launchpad program** (e.g. the Pump.fun program) → **Info**.
  Bonding-curve custody: the program, not a person, controls minting under
  its own rules until graduation. This is a design property, not a red
  flag — but you should know it exists.
- Held by any **other wallet** → **Critical**. Evidence shows the exact
  authority pubkey; paste it into an explorer to see who it is.

**False-positive rate.** Low for the Critical case — an active wallet
authority is an objective fact. The Info case (launchpad custody) is
deliberately not scored as risk; disagree by reading the program's own
rules. Known blind spot: we cannot tell *who* controls a program's
upgrade authority, so "the program holds it" is only as trustworthy as the
program.

**Verify yourself.** On Solscan or SolanaFM, open the token's mint account:
"Mint Authority" and "Freeze Authority" should read (unless launchpad
custody applies).

## S2. LP tokens not burned or locked

**What it is.** For DEX-paired tokens, someone receives *liquidity
provider* tokens representing the pooled funds. If the deployer keeps
them, they can withdraw the entire pool — the classic "liquidity pull".

**Why it matters.** Locked or burned LP is the closest thing on-chain to a
commitment that the market stays funded.

**Status in v1: not yet assessed.** Verifying this needs pool-state parsing
per DEX layout (each has a different account format) plus burn/lock
program identification. The analysis records this signal as "not assessed —
planned with the enrichment phase" rather than guessing. Guessing here
would be worse than silence: a wrong "LP looks fine" is the most dangerous
output a tool like this can produce.

**False-positive rate.** Not stated until the check exists.

## S3. Fresh or mixer-funded dev wallet

**What it is.** Who funded the deployer wallet, and when? A wallet created
and funded an hour before launch — especially through a mixing service that
breaks the money trail — is a classic anonymous-rug setup.

**Why it matters.** Rugs are crimes of opportunity; anonymity is the tool.

**Status in v1: not yet assessed.** Needs wallet-history enrichment
(`getSignaturesForAddress` walks plus funding-transaction inspection) — the
rate-limited public endpoint makes this expensive at stream volume, which
is exactly the problem a dedicated provider solves. Planned with the
Helius integration.

**False-positive rate.** Will be stated once measured; expect meaningful
false positives from exchange hot wallets funding legitimate deployers.

## S4. Dev with a history of launches

**What it is.** The same wallet repeatedly launching tokens that collapse
is the strongest single predictor of a future rug.

**Why it matters.** Serial launchers optimize for speed; track record is
the only reputation that exists on-chain.

**How we check.** The creation transaction identifies the wallet that paid
for it (the deployer). TrenchWatch then looks that wallet up in its own
stored analyses — the receipts this tool has already collected, nothing
from a third-party score. Prior launches attributed to the same wallet are
summarized in the receipt, and the signal is graded:

- any prior launch raised **critical** flags → **Warn** (a pattern of
  flagged launches is riskier than any single launch suggests)
- prior launches exist, none critical → **Info** (known history, no
  repeat-offender pattern — noted, not celebrated)
- the wallet is first-seen → recorded as *not assessed*, with the reason
  stated plainly, never treated as a clean bill

**Limitations.** History only exists for what this tool observed: a wallet
that launched elsewhere before you started watching looks "first-seen"
here. Reputation is local, and the receipt says so.

**False-positive rate.** Not stated until measured. Watch for deployer
rotation (a pro uses a fresh wallet per launch — S3's mixer/freshness
signal is the counterweight).

## S5. Holder concentration

**What it is.** How much of the total supply the largest token accounts
control.

**Why it matters.** Concentrated supply means a small group can crash the
price by selling in tandem — and often means the "liquidity" is an
illusion, with insiders holding most of the float.

**How we check.** `getTokenLargestAccounts` (top 20 token accounts) summed
as a percentage of total supply (from the mint account). Thresholds:
top-10 together above **50%** → **Warn**; above **80%** → **Critical**.
The receipt lists every counted account with its percentage so you can
inspect them individually.

**False-positive rate.** Moderate, and we say so in the flag text: the
largest accounts can include the pool itself, lockup contracts, or the
launchpad's own custody accounts — all benign. A 70% "top 10" on a
launchpad-custody token may be mostly the bonding curve. *Always read the
account list in the evidence before concluding.* Measured rates: pending a
soak run; this entry will be updated with real numbers.

**Verify yourself.** Solscan → token page → "Holders". Compare the top
addresses against the pool and custody accounts.

## S6. Thin liquidity

**What it is.** How much real money backs the token: single-sided pools,
near-empty quote-vault balances, or a bonding curve barely past creation.

**Why it matters.** Thin liquidity means tiny sells crater the price —
and makes price manipulation cheap.

**Status in v1: not yet assessed.** Needs pool-balance reads and per-DEX
pool math; planned with the enrichment phase, alongside S2 (they share the
pool-state parsing work).

**False-positive rate.** Not stated until measured.

---

## Notes from live testing

Real-mainnet observations baked into v1 behavior, so you are not surprised
when you see them in the stream:

- **A brand-new mint has no token accounts yet.** At the moment of launch
  there is nothing to hold it, so `getTokenLargestAccounts` returns empty
  and S5 honestly reports *not assessed* instead of showing a fake 0%.
  Concentration becomes assessable a few seconds later, once buys create
  token accounts.
- **Not every launch carries its token's creation transaction.** Some
  frontends bundle pool creation into transactions that reference the token
  but never create it. When that happens the engine reports the mint as
  unknown rather than guessing.
- **Liquidity pools always involve wrapped SOL**, so the engine deliberately
  ignores WSOL when identifying "the new token" — it would otherwise name
  SOL as the launched asset on some pools.

## How a verdict is composed

Each launch gets: every flag that fired (with evidence), every signal that
could not be assessed (with the reason — never a silent hole), and a
derived one-line summary. The summary follows one rule: it states what was
and wasn't found *as of a slot*, and never says "safe".

## Improving these numbers

If you watch the stream and notice a flag that was wrong, open an issue with
the signature. Every wrong flag is a fixture for the test suite — measured
false-positive rates only get real by collecting cases.
