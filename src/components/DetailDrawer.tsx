import { absoluteTime } from "../lib/format";
import { SIGNAL_LABELS, type LaunchAnalysis, type Severity } from "../lib/types";
import { verdictText } from "../lib/verdict";

const SEVERITY_LABEL: Record<Severity, string> = {
  critical: "CRITICAL",
  warn: "WARNING",
  info: "NOTE",
};

export default function DetailDrawer({
  analysis,
  onClose,
}: {
  analysis: LaunchAnalysis;
  onClose: () => void;
}) {
  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer" onClick={(event) => event.stopPropagation()}>
        <header className="drawer-header">
          <div>
            <h2>{analysis.mint ? "Token launch analysis" : "Launch analysis"}</h2>
            <p className="drawer-verdict">{verdictText(analysis)}</p>
          </div>
          <button className="drawer-close" onClick={onClose} aria-label="Close details">
            ✕
          </button>
        </header>

        <section className="drawer-section">
          <h3>Identity</h3>
          <dl className="kv">
            <dt>signature</dt>
            <dd className="mono wrap">{analysis.signature}</dd>
            <dt>slot</dt>
            <dd className="mono">{analysis.slot.toLocaleString()}</dd>
            <dt>token mint</dt>
            <dd className="mono wrap">{analysis.mint ?? "not identified from the creation transaction"}</dd>
            <dt>dev wallet</dt>
            <dd className="mono wrap">{analysis.dev_wallet ?? "not identified from the creation transaction"}</dd>
            <dt>assessed</dt>
            <dd>{absoluteTime(analysis.assessed_at_unix_ms)}</dd>
          </dl>
        </section>

        {analysis.graduation && (
          <section className="drawer-section">
            <h3>🎓 Graduated to {analysis.graduation.venue_label}</h3>
            <p className="graduation-note">
              This token&apos;s bonding-curve phase ended and its liquidity moved to a{" "}
              {analysis.graduation.venue_label} pool — the on-chain migration that makes it
              tradeable on a real DEX. TrenchWatch detected it because the pool-creation
              transaction <em>is</em> the migration: same token mint, same transaction, so the
              link is cryptographic, not a guess.
            </p>
            <dl className="kv">
              <dt>graduated at slot</dt>
              <dd className="mono">{analysis.graduation.slot.toLocaleString()}</dd>
              <dt>migration transaction</dt>
              <dd className="mono wrap">{analysis.graduation.signature}</dd>
              <dt>observed</dt>
              <dd>{absoluteTime(analysis.graduation.detected_unix_ms)}</dd>
            </dl>
            <p className="dim">
              Centralized-exchange listings (MEXC, Bybit, …) are not announced on-chain, so
              they cannot be observed from chain data and are not tracked.
            </p>
          </section>
        )}

        {analysis.flags.length > 0 && (
          <section className="drawer-section">
            <h3>Risk flags — each with its on-chain receipt</h3>
            {analysis.flags.map((flag, index) => (
              <article key={`${flag.signal}-${index}`} className={`flag-card sev-${flag.severity}`}>
                <header className="flag-head">
                  <span className={`chip chip-${flag.severity === "critical" ? "critical" : flag.severity === "warn" ? "warn" : "info"}`}>
                    {SEVERITY_LABEL[flag.severity]}
                  </span>
                  <span className="flag-signal">{SIGNAL_LABELS[flag.signal]}</span>
                </header>
                <p className="flag-explanation">{flag.explanation}</p>
                <details className="flag-receipt">
                  <summary>Receipt · observed slot {flag.slot.toLocaleString()}</summary>
                  <pre className="mono">{JSON.stringify(flag.evidence, null, 2)}</pre>
                </details>
                <p className="flag-ref mono">{flag.field_guide_ref}</p>
              </article>
            ))}
          </section>
        )}

        {analysis.not_assessed.length > 0 && (
          <section className="drawer-section">
            <h3>Not yet assessed — and why</h3>
            <ul className="not-assessed">
              {analysis.not_assessed.map((entry, index) => (
                <li key={`${entry.signal}-${index}`}>
                  <span className="na-signal">{SIGNAL_LABELS[entry.signal]}</span>
                  <span className="na-reason">{entry.reason}</span>
                </li>
              ))}
            </ul>
          </section>
        )}

        <footer className="drawer-footer">
          Every verdict above is derived from the receipts shown — nothing here is a bare score.
          A "no flags found" result is an observation as of slot {analysis.slot.toLocaleString()},
          not a guarantee of safety.
        </footer>
      </aside>
    </div>
  );
}
