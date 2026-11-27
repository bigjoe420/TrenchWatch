import { invoke } from "@tauri-apps/api/core";

import type {
  AlertConfig,
  LaunchAnalysis,
  LaunchEvent,
  RpcSetupStatus,
  StatusResponse,
} from "./types";

export async function getStatus(): Promise<StatusResponse> {
  return invoke<StatusResponse>("get_status");
}

export async function getRecentAnalyses(limit = 200): Promise<LaunchAnalysis[]> {
  return invoke<LaunchAnalysis[]>("get_recent_analyses", { limit });
}

export async function getRecentEvents(limit = 200): Promise<LaunchEvent[]> {
  const response = await invoke<{ events: LaunchEvent[]; total_stored: number }>(
    "get_recent_events",
    { limit },
  );
  return response.events;
}

export async function getAnalysis(signature: string): Promise<LaunchAnalysis | null> {
  return invoke<LaunchAnalysis | null>("get_analysis", { signature });
}

export async function getAlertsConfig(): Promise<AlertConfig> {
  return invoke<AlertConfig>("get_alerts_config");
}

export async function updateAlertsConfig(config: AlertConfig): Promise<void> {
  return invoke<void>("update_alerts_config", { alerts: config });
}

export async function getRpcSetupStatus(): Promise<RpcSetupStatus> {
  return invoke<RpcSetupStatus>("get_rpc_setup_status");
}

/** Write-only: the key goes to the backend and is never read back. */
export async function setApiKey(apiKey: string): Promise<void> {
  return invoke<void>("set_api_key", { apiKey });
}

/** The public TrenchWatch alerts server — same invite as the README. */
export const DISCORD_INVITE = "https://discord.gg/SNjAp6FQB";

export async function openUrl(url: string): Promise<void> {
  return invoke<void>("open_url", { url });
}
