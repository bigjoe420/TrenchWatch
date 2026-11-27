// Mirrors the serde output of src-tauri/src/risk.rs and commands.rs.
// Keep in sync — these types are the IPC contract with the backend.

export type Severity = "info" | "warn" | "critical";

export type SignalKind =
  | "mint_freeze_authority"
  | "lp_burn_or_lock"
  | "dev_fresh_funding"
  | "dev_repeat_launcher"
  | "holder_concentration"
  | "thin_liquidity";

export interface RiskFlag {
  signal: SignalKind;
  severity: Severity;
  explanation: string;
  evidence: Record<string, unknown>;
  field_guide_ref: string;
  slot: number;
  observed_unix_ms: number;
}

export interface NotAssessed {
  signal: SignalKind;
  reason: string;
}

export interface LaunchAnalysis {
  signature: string;
  slot: number;
  mint: string | null;
  flags: RiskFlag[];
  not_assessed: NotAssessed[];
  assessed_at_unix_ms: number;
}

export interface LaunchEvent {
  signature: string;
  slot: number;
  program: string;
  program_label: string;
  logs: string[];
  detected_unix_ms: number;
}

export interface StatusResponse {
  listener: string;
  events_stored: number;
  events_dropped: number;
  uptime_secs: number;
  endpoint: string;
}

export const SIGNAL_LABELS: Record<SignalKind, string> = {
  mint_freeze_authority: "Mint & freeze authority",
  lp_burn_or_lock: "LP burn / lock",
  dev_fresh_funding: "Dev funding source",
  dev_repeat_launcher: "Dev launch history",
  holder_concentration: "Holder concentration",
  thin_liquidity: "Liquidity depth",
};
