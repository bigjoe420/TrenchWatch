import { useEffect, useState } from "react";

import { getRpcSetupStatus, setApiKey, updateAlertsConfig } from "../lib/ipc";
import type { AlertConfig, Severity } from "../lib/types";

export default function SettingsDrawer({
  initial,
  onClose,
}: {
  initial: AlertConfig;
  onClose: () => void;
}) {
  const [config, setConfig] = useState<AlertConfig>(initial);
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  const [keyOnFile, setKeyOnFile] = useState<boolean | null>(null);
  const [status, setStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState("");

  useEffect(() => {
    getRpcSetupStatus()
      .then((s) => setKeyOnFile(s.api_key_configured))
      .catch(() => setKeyOnFile(null));
  }, []);

  const save = async () => {
    setStatus("saving");
    setError("");
    try {
      if (apiKeyDraft.trim().length > 0) {
        await setApiKey(apiKeyDraft); // write-only; applies live, never read back
        setKeyOnFile(apiKeyDraft.trim().length > 0);
        setApiKeyDraft("");
      }
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
            <h2>Settings</h2>
            <p className="drawer-verdict">
              Helius key, alert routing, and thresholds — saved locally, applied immediately.
            </p>
          </div>
          <button className="drawer-close" onClick={onClose} aria-label="Close settings">
            ✕
          </button>
        </header>

        <section className="drawer-section">
          <h3>Helius</h3>
          <label className="settings-row">
            <span className="settings-label">api key</span>
            <input
              type="password"
              className="settings-input mono"
              placeholder="leave empty to keep the current key"
              value={apiKeyDraft}
              onChange={(e) => setApiKeyDraft(e.target.value)}
              spellCheck={false}
            />
            <em className="settings-hint">
              {keyOnFile === null
                ? "Key status unknown."
                : keyOnFile
                  ? "A key is on file — enrichment is active. Paste above to replace it."
                  : "No key on file — analysis reads use the public endpoint. Paste a key to enable Helius enrichment."}{" "}
              Write-only: it is never displayed or read back.
            </em>
          </label>
        </section>

        <section className="drawer-section">
          <h3>Alerts</h3>
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
