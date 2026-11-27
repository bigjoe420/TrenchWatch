import { useState } from "react";

import { updateAlertsConfig } from "../lib/ipc";
import type { AlertConfig, Severity } from "../lib/types";

export default function SettingsDrawer({
  initial,
  onClose,
}: {
  initial: AlertConfig;
  onClose: () => void;
}) {
  const [config, setConfig] = useState<AlertConfig>(initial);
  const [status, setStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState("");

  const save = async () => {
    setStatus("saving");
    setError("");
    try {
      await updateAlertsConfig(config);
      setStatus("saved");
    } catch (e) {
      setStatus("error");
      setError(e instanceof Error ? e.message : "could not save settings");
    }
  };

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer drawer-narrow" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-header">
          <div>
            <h2>Alerts</h2>
            <p className="drawer-verdict">
              Get notified when a launch is flagged at or above your chosen severity.
            </p>
          </div>
          <button className="drawer-close" onClick={onClose} aria-label="Close settings">
            ✕
          </button>
        </header>

        <section className="drawer-section">
          <label className="settings-row settings-toggle">
            <input
              type="checkbox"
              checked={config.desktop_notifications}
              onChange={(e) =>
                setConfig((c) => ({ ...c, desktop_notifications: e.target.checked }))
              }
            />
            <span>
              <strong>Desktop notifications</strong>
              <em>A system notification for every flagged launch that meets the threshold.</em>
            </span>
          </label>

          <label className="settings-row">
            <span className="settings-label">alert threshold</span>
            <select
              value={config.min_severity}
              onChange={(e) =>
                setConfig((c) => ({ ...c, min_severity: e.target.value as Severity }))
              }
            >
              <option value="critical">critical only</option>
              <option value="warn">warnings and above</option>
              <option value="info">every flag</option>
            </select>
          </label>

          <label className="settings-row settings-toggle">
            <input
              type="checkbox"
              checked={config.graduation_alerts}
              onChange={(e) =>
                setConfig((c) => ({ ...c, graduation_alerts: e.target.checked }))
              }
            />
            <span>
              <strong>Graduation alerts</strong>
              <em>
                Announce when a tracked token graduates to a real DEX pool (e.g. Pump.fun →
                PumpSwap) — even if no risk flag fired. Lifecycle news, not a verdict.
              </em>
            </span>
          </label>

          <label className="settings-row">
            <span className="settings-label">discord webhook</span>
            <input
              type="text"
              className="settings-input mono"
              placeholder="https://discord.com/api/webhooks/… (empty = disabled)"
              value={config.discord_webhook_url}
              onChange={(e) => setConfig((c) => ({ ...c, discord_webhook_url: e.target.value }))}
              spellCheck={false}
            />
            <em className="settings-hint">
              Receipts-style alert embeds are posted to this URL only — nothing else is sent
              anywhere.
            </em>
          </label>

          <label className="settings-row">
            <span className="settings-label">critical-flags webhook</span>
            <input
              type="text"
              className="settings-input mono"
              placeholder="https://discord.com/api/webhooks/… (empty = same as above)"
              value={config.discord_critical_webhook_url}
              onChange={(e) =>
                setConfig((c) => ({ ...c, discord_critical_webhook_url: e.target.value }))
              }
              spellCheck={false}
            />
            <em className="settings-hint">
              Flag alerts post here instead; graduation alerts always go to the webhook above.
            </em>
          </label>
        </section>

        <div className="settings-actions">
          <button className="settings-save" onClick={save} disabled={status === "saving"}>
            {status === "saving" ? "saving…" : "save"}
          </button>
          {status === "saved" && <span className="settings-saved">saved — takes effect immediately</span>}
          {status === "error" && <span className="settings-error">{error}</span>}
        </div>

        <footer className="drawer-footer">
          Settings live in <code>config.toml</code> next to the app data directory — local,
          gitignored, never uploaded.
        </footer>
      </aside>
    </div>
  );
}
