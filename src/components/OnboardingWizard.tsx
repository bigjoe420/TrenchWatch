import { useEffect, useState } from "react";

import {
  DISCORD_INVITE,
  getAlertsConfig,
  getRpcSetupStatus,
  openUrl,
  setApiKey,
  updateAlertsConfig,
} from "../lib/ipc";
import type { AlertConfig } from "../lib/types";

/**
 * First-run setup wizard. Shown on a fresh install (setup_completed = false)
 * before the dashboard takes over. Two paste-ins — a Helius API key and an
 * optional Discord webhook — both optional, both skippable, both editable
 * later in Settings. Finishing sets setup_completed and never shows again.
 */
export default function OnboardingWizard({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [apiKey, setApiKeyInput] = useState("");
  const [apiKeyDone, setApiKeyDone] = useState(false);
  const [webhook, setWebhook] = useState("");
  const [criticalWebhook, setCriticalWebhook] = useState("");
  const [webhookDone, setWebhookDone] = useState(false);
  const [alerts, setAlerts] = useState<AlertConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getRpcSetupStatus()
      .then((status) => setApiKeyDone(status.api_key_configured))
      .catch(() => {});
    getAlertsConfig()
      .then((config) => {
        setAlerts(config);
        setWebhook(config.discord_webhook_url);
        setCriticalWebhook(config.discord_critical_webhook_url);
        setWebhookDone(config.discord_webhook_url.trim().length > 0);
      })
      .catch(() => {});
  }, []);

  const saveKey = async () => {
    setSaving(true);
    setError(null);
    try {
      await setApiKey(apiKey);
      setApiKeyDone(true);
      setStep(2);
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not save the api key");
    } finally {
      setSaving(false);
    }
  };

  const saveWebhook = async () => {
    if (!alerts) return;
    setSaving(true);
    setError(null);
    try {
      const next = {
        ...alerts,
        discord_webhook_url: webhook.trim(),
        discord_critical_webhook_url: criticalWebhook.trim(),
      };
      await updateAlertsConfig(next);
      setAlerts(next);
      setWebhookDone(webhook.trim().length > 0);
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not save the webhooks");
    } finally {
      setSaving(false);
    }
  };

  const finish = async () => {
    if (!alerts) return;
    setSaving(true);
    setError(null);
    try {
      await updateAlertsConfig({ ...alerts, setup_completed: true });
      onDone();
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not finish setup");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="wizard-overlay">
      <div className="wizard-card">
        <header className="wizard-header">
          <span className="wizard-logo">TRENCHWATCH</span>
          <span className="wizard-steps">
            {["welcome", "helius key", "discord", "done"].map((label, i) => (
              <span
                key={label}
                className={i === step ? "wizard-step active" : i < step ? "wizard-step done" : "wizard-step"}
              >
                {label}
              </span>
            ))}
          </span>
        </header>

        {step === 0 && (
          <section className="wizard-body">
            <h1>watch the trench before it watches you</h1>
            <p>
              TrenchWatch follows every token launched on pump.fun, flags the risky ones at
              birth, and announces each launch that graduates to a real DEX — straight into
              Discord. Two quick paste-ins and you&apos;re live.
            </p>
            <div className="wizard-actions">
              <button className="wizard-primary" onClick={() => setStep(1)}>
                let&apos;s set it up
              </button>
            </div>
          </section>
        )}

        {step === 1 && (
          <section className="wizard-body">
            <h1>your Helius API key</h1>
            <p>
              The launch stream is free and needs nothing, but a Helius key unlocks the
              paid-read enrichment that sharpens every verdict (parsed transactions, mint
              accounts, holder distribution). Free tier works fine.
            </p>
            {apiKeyDone && (
              <p className="wizard-note">a key is already on file — you can keep it or replace it.</p>
            )}
            <input
              type="password"
              className="wizard-input mono"
              placeholder="paste your Helius API key (or leave empty and skip)"
              value={apiKey}
              onChange={(e) => setApiKeyInput(e.target.value)}
              spellCheck={false}
              autoFocus
            />
            {error && <p className="wizard-error">{error}</p>}
            <div className="wizard-actions">
              <button className="wizard-link" onClick={() => void openUrl("https://helius.dev")}>
                get a key at helius.dev ↗
              </button>
              <span className="wizard-spacer" />
              <button className="wizard-link" onClick={() => setStep(2)}>
                {apiKeyDone ? "keep existing key" : "skip for now"}
              </button>
              <button className="wizard-primary" onClick={saveKey} disabled={saving}>
                {saving ? "saving…" : apiKey ? "save key" : "continue without a key"}
              </button>
            </div>
          </section>
        )}

        {step === 2 && (
          <section className="wizard-body">
            <h1>your Discord webhooks</h1>
            <p>
              Alerts post to your own server through webhooks — the app talks to nothing
              else. To make one: Server Settings → Integrations → Webhooks → New Webhook →
              pick the channel → Copy Webhook URL.
            </p>
            {webhookDone && (
              <p className="wizard-note">webhooks are already on file — you can keep or replace them.</p>
            )}
            <em className="wizard-label">main channel — every launch receipt + graduation announcements</em>
            <input
              type="text"
              className="wizard-input mono"
              placeholder="https://discord.com/api/webhooks/… (empty = alerts stay off)"
              value={webhook}
              onChange={(e) => setWebhook(e.target.value)}
              spellCheck={false}
            />
            <em className="wizard-label">critical flags — posted here instead (optional, same channel is fine)</em>
            <input
              type="text"
              className="wizard-input mono"
              placeholder="https://discord.com/api/webhooks/… (empty = uses the main webhook)"
              value={criticalWebhook}
              onChange={(e) => setCriticalWebhook(e.target.value)}
              spellCheck={false}
            />
            {error && <p className="wizard-error">{error}</p>}
            <div className="wizard-actions">
              <button className="wizard-link" onClick={() => void openUrl(DISCORD_INVITE)}>
                or join the TrenchWatch server ↗
              </button>
              <span className="wizard-spacer" />
              <button className="wizard-link" onClick={() => setStep(3)}>
                skip — alerts off
              </button>
              <button className="wizard-primary" onClick={saveWebhook} disabled={saving || !alerts}>
                {saving ? "saving…" : "save webhook"}
              </button>
            </div>
          </section>
        )}

        {step === 3 && (
          <section className="wizard-body">
            <h1>you&apos;re in the trench</h1>
            <p>
              {apiKeyDone
                ? "Helius enrichment is live and the meter is running. "
                : "You can add a Helius key later in Settings. "}
              {webhookDone
                ? "Alerts will hit your Discord the moment something graduates or trips a flag."
                : "Discord alerts are off — add a webhook in Settings whenever you want them."}{" "}
              The stream is already watching.
            </p>
            {error && <p className="wizard-error">{error}</p>}
            <div className="wizard-actions">
              <span className="wizard-spacer" />
              <button className="wizard-primary" onClick={finish} disabled={saving}>
                {saving ? "saving…" : "open the dashboard"}
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
