# Contributing to TrenchWatch

Thanks for helping. This project is a teaching artifact as much as a tool —
code here will be read by people learning Solana data tooling, so clarity
beats cleverness.

## Ground rules

1. **Read-only by design.** No code that trades, signs, holds keys, or
   custodies wallets — ever. See `docs/THREAT_MODEL.md` §2. Dependency
   additions are reviewed against it.
2. **No black-box scores.** Every risk signal must emit
   `{flag, severity, plain-language explanation, on-chain evidence,
   field-guide reference}`. If you can't show the receipt, don't ship the
   flag.
3. **Conventional commits** from the first commit:
   `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:` — scope optional
   (`feat(risk): flag LP tokens held by deployer`).
4. **Rust:** `cargo clippy` clean, no `.unwrap()` in production paths,
   errors via `anyhow`/`thiserror`. Risk-engine changes ship with unit tests
   + fixtures (sanitized real on-chain examples).
5. **Frontend:** TypeScript strict, hand-rolled CSS using the theme tokens
   in `src/styles/global.css`, no remote assets, no `dangerouslySetInnerHTML`.

## Setup

```bash
npm install
npm run tauri dev
```

## Pull requests

- Small and reviewable. One concern per PR.
- Include what evidence a reviewer can check (test output, log excerpt,
  slot number for on-chain claims).
- `npm run build` (frontend type-check) and `cargo clippy` must pass.

## Reporting security issues

Open a private advisory via GitHub **Security → Advisories** rather than a
public issue if the report involves a live vulnerability.
