import { formatDuration } from "../lib/format";
import type { StatusResponse } from "../lib/types";

// Listener state wording comes from the backend verbatim (plain language by
// design); here we only map it to a visual state.
function stateOf(status: StatusResponse | null): "live" | "connecting" | "degraded" {
  if (!status) return "connecting";
  const text = status.listener.toLowerCase();
  if (text.includes("dropped") || text.includes("stopped")) return "degraded";
  if (text.includes("watching") || text.includes("connected")) return "live";
  return "connecting";
}

export default function TopBar({
  status,
  onOpenSettings,
}: {
  status: StatusResponse | null;
  onOpenSettings: () => void;
}) {
  const state = stateOf(status);
  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="brand">
          TRENCH<span className="brand-gradient">WATCH</span>
        </span>
        <span className="brand-sub">Solana launch monitor</span>
      </div>

      <div className="topbar-right">
        {status && (
          <>
            <span className="stat">
              <span className="stat-label">stored</span>
              <span className="stat-value">{status.events_stored}</span>
            </span>
            <span className="stat">
              <span className="stat-label">dropped</span>
              <span className={`stat-value ${status.events_dropped > 0 ? "stat-warn" : ""}`}>
                {status.events_dropped}
              </span>
            </span>
            <span className="stat">
              <span className="stat-label">uptime</span>
              <span className="stat-value">{formatDuration(status.uptime_secs)}</span>
            </span>
            {status.helius.enabled && (
              <span className="stat" title="Paid Helius enrichment calls used today (reads only; the launch stream stays on the free websocket)">
                <span className="stat-label">helius</span>
                <span className="stat-value">
                  {status.helius.used_today.toLocaleString()}
                  {status.helius.daily_cap > 0
                    ? ` / ${status.helius.daily_cap.toLocaleString()}`
                    : " / ∞"}
                </span>
              </span>
            )}
          </>
        )}
        <span className={`status-pill status-${state}`}>
          <span className="status-dot" />
          {status ? status.listener : "connecting…"}
        </span>
        <button className="icon-button" onClick={onOpenSettings} aria-label="Alert settings" title="Alert settings">
          ⚙
        </button>
      </div>
    </header>
  );
}
