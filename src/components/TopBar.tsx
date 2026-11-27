import { DISCORD_INVITE, openUrl } from "../lib/ipc";
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
        {/* First in the row so the cluster can never push the only
            interactive control off the window's right edge. */}
        <button className="icon-button" onClick={onOpenSettings} aria-label="Alert settings" title="Alert settings">
          ⚙
        </button>
        <button
          className="icon-button icon-discord"
          onClick={() => void openUrl(DISCORD_INVITE).catch(() => {})}
          aria-label="Join the TrenchWatch alerts server on Discord"
          title="Join the live alerts server — watch graduations and flags fire in real time"
        >
          {/* Discord glyph (simple path, currentColor) */}
          <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
            <path d="M20.3 4.4A19.8 19.8 0 0 0 15.4 3c-.2.4-.5.9-.6 1.2a18.3 18.3 0 0 0-5.5 0C9.1 3.9 8.8 3.4 8.6 3a19.7 19.7 0 0 0-4.9 1.5A20.4 20.4 0 0 0 .2 18.1a19.9 19.9 0 0 0 6 3c.5-.7.9-1.4 1.3-2.1-.7-.3-1.4-.6-2-1l.5-.4a14.2 14.2 0 0 0 12.2 0l.5.4c-.7.4-1.4.7-2.1 1 .4.7.8 1.4 1.3 2.1a19.8 19.8 0 0 0 6-3A20.3 20.3 0 0 0 20.3 4.4ZM8 15.3c-1.2 0-2.1-1.1-2.1-2.4S6.8 10.5 8 10.5s2.2 1.1 2.1 2.4c0 1.3-.9 2.4-2.1 2.4Zm8 0c-1.2 0-2.1-1.1-2.1-2.4s.9-2.4 2.1-2.4 2.2 1.1 2.1 2.4c0 1.3-.9 2.4-2.1 2.4Z" />
          </svg>
        </button>
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
              <span className="stat stat-helius" title="Paid Helius enrichment calls used today (reads only; the launch stream stays on the free websocket)">
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
        <span
          className={`status-pill status-${state}`}
          title={status ? status.listener : "connecting to mainnet"}
        >
          <span className="status-dot" />
          <span className="status-pill-text">
            {status ? status.listener : "connecting…"}
          </span>
        </span>
      </div>
    </header>
  );
}
