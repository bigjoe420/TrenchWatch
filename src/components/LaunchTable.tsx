import { shortAddress, timeAgo } from "../lib/format";
import type { LaunchAnalysis, LaunchEvent, Severity } from "../lib/types";
import { maxSeverity } from "../lib/verdict";

export interface TableFilters {
  program: string; // "all" or an event program id
  severity: "all" | Severity | "none";
  /** "all" = everything; "launches" = not yet graduated; "graduated" = new on DEXs. */
  stage: "all" | "launches" | "graduated";
  hideFlagged: boolean;
}

const SEVERITY_ORDER: Record<string, number> = { critical: 0, warn: 1, info: 2 };

function severityLabel(analysis: LaunchAnalysis): { label: string; cls: string } {
  const severity = maxSeverity(analysis);
  switch (severity) {
    case "critical":
      return { label: "CRITICAL", cls: "chip-critical" };
    case "warn":
      return { label: "WARNING", cls: "chip-warn" };
    case "info":
      return { label: "NOTE", cls: "chip-info" };
    default:
      return { label: "NO FLAGS FOUND", cls: "chip-none" };
  }
}

interface Row {
  analysis: LaunchAnalysis;
  programLabel: string | null;
}

export default function LaunchTable({
  analyses,
  events,
  filters,
  selected,
  onSelect,
}: {
  analyses: LaunchAnalysis[];
  events: LaunchEvent[];
  filters: TableFilters;
  selected: string | null;
  onSelect: (signature: string) => void;
}) {
  const programBySignature = new Map(events.map((event) => [event.signature, event.program_label]));

  const rows: Row[] = analyses
    .map((analysis) => ({
      analysis,
      programLabel: programBySignature.get(analysis.signature) ?? null,
    }))
    .filter(({ analysis, programLabel }) => {
      if (filters.program !== "all" && programLabel !== filters.program) return false;
      if (filters.severity !== "all") {
        const severity = maxSeverity(analysis);
        if (filters.severity === "none") {
          if (severity !== null) return false;
        } else if (severity !== filters.severity) {
          return false;
        }
      }
      if (filters.hideFlagged && analysis.flags.length > 0) return false;
      if (filters.stage === "graduated" && !analysis.graduation) return false;
      if (filters.stage === "launches" && analysis.graduation) return false;
      return true;
    });

  // Newest first within a severity band, so critical launches rise to the top.
  const rank = (row: Row) => {
    const severity = maxSeverity(row.analysis);
    const sev = severity === null ? 3 : SEVERITY_ORDER[severity];
    return sev * 1e15 + row.analysis.slot;
  };
  rows.sort((a, b) => rank(b) - rank(a));

  return (
    <div className="table-wrap">
      <table className="launch-table">
        <thead>
          <tr>
            <th>verdict</th>
            <th>token mint</th>
            <th>launchpad</th>
            <th>slot</th>
            <th>signals</th>
            <th>assessed</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ analysis, programLabel }) => {
            const severity = severityLabel(analysis);
            const counts = {
              critical: analysis.flags.filter((f) => f.severity === "critical").length,
              warn: analysis.flags.filter((f) => f.severity === "warn").length,
              info: analysis.flags.filter((f) => f.severity === "info").length,
            };
            return (
              <tr
                key={analysis.signature}
                className={selected === analysis.signature ? "row-selected" : ""}
                onClick={() => onSelect(analysis.signature)}
              >
                <td>
                  <span className={`chip ${severity.cls}`}>{severity.label}</span>
                </td>
                <td className="mono-cell" title={analysis.mint ?? "mint not identified from the creation transaction"}>
                  {analysis.mint ? shortAddress(analysis.mint) : "mint unknown"}
                </td>
                <td>
                  {programLabel ?? "—"}
                  {analysis.graduation && (
                    <span
                      className="chip chip-grad"
                      title={`Graduated to ${analysis.graduation.venue_label} at slot ${analysis.graduation.slot.toLocaleString()} — migration tx ${analysis.graduation.signature}`}
                    >
                      🎓 {analysis.graduation.venue_label}
                    </span>
                  )}
                </td>
                <td className="mono-cell">{analysis.slot.toLocaleString()}</td>
                <td>
                  <span className="signal-counts">
                    {counts.critical > 0 && <span className="cnt cnt-critical">{counts.critical} critical</span>}
                    {counts.warn > 0 && <span className="cnt cnt-warn">{counts.warn} warning</span>}
                    {counts.info > 0 && <span className="cnt cnt-info">{counts.info} note</span>}
                    {analysis.flags.length === 0 && (
                      <span className="cnt cnt-dim">{analysis.not_assessed.length} unassessed</span>
                    )}
                  </span>
                </td>
                <td className="dim-cell">{timeAgo(analysis.assessed_at_unix_ms)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && analyses.length === 0 && (
        <div className="empty-state">
          <p>Radar is live — watching Solana mainnet for new pool creations.</p>
          <p className="dim">
            Launches appear here within seconds during active hours, each with a full
            evidence receipt. Nothing yet is normal: new launches pause overnight and on
            quiet weekends — leave it running. If this stays empty for a long stretch,
            check the status pill in the top right.
          </p>
          <p className="dim">
            New here? The gear icon (top right) sets up alerts — desktop notifications
            and Discord — and an optional Helius key for faster analysis reads.
            Everything works without them.
          </p>
        </div>
      )}
      {rows.length === 0 && analyses.length > 0 && (
        <div className="empty-state">
          <p>No launches match these filters.</p>
          <p className="dim">
            Try another launchpad or verdict — or uncheck “hide flagged launches” to see
            everything TrenchWatch has analyzed.
          </p>
        </div>
      )}
    </div>
  );
}
