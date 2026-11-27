import type { LaunchAnalysis, Severity } from "./types";

// Mirrors risk::LaunchAnalysis::verdict_text in src-tauri/src/risk.rs.
// Derived client-side from flags + not_assessed so the summary can never
// drift from the receipts it claims to summarize.

export function maxSeverity(analysis: LaunchAnalysis): Severity | null {
  if (analysis.flags.length === 0) return null;
  if (analysis.flags.some((flag) => flag.severity === "critical")) return "critical";
  if (analysis.flags.some((flag) => flag.severity === "warn")) return "warn";
  return "info";
}

export function verdictText(analysis: LaunchAnalysis): string {
  if (analysis.flags.length === 0) {
    return `No risk flags found as of slot ${analysis.slot} — not a guarantee of safety. ${analysis.not_assessed.length} signal(s) not yet assessed.`;
  }
  const count = (severity: Severity) =>
    analysis.flags.filter((flag) => flag.severity === severity).length;
  const parts: string[] = [];
  if (count("critical") > 0) parts.push(`${count("critical")} critical`);
  if (count("warn") > 0) parts.push(`${count("warn")} warning`);
  if (count("info") > 0) parts.push(`${count("info")} note`);
  const pending = analysis.not_assessed.length;
  return `${parts.join(", ")} as of slot ${analysis.slot}.${pending > 0 ? ` ${pending} signal(s) not yet assessed.` : ""}`;
}
