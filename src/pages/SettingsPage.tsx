import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, Field, Input, Select } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

const CLOUD: Record<api.LlmProvider, boolean> = {
  anthropic: true, openai: true, google: true, ollama: false, custom: true,
};
const USES_KEY: Record<api.LlmProvider, boolean> = {
  anthropic: true, openai: true, google: true, ollama: false, custom: true,
};
const LABEL: Record<api.LlmProvider, string> = {
  anthropic: "Anthropic (Claude)", openai: "OpenAI", google: "Google (Gemini)",
  ollama: "Ollama (local)", custom: "Custom (OpenAI-compatible)",
};

// Rebuild the full LlmSettings the backend expects from the array-shaped view.
function toSettings(view: api.LlmSettingsView): api.LlmSettings {
  const byProvider = (p: api.LlmProvider) =>
    view.providers.find((x) => x.provider === p)!.config;
  return {
    active_provider: view.active_provider,
    cloud_egress_ack: view.cloud_egress_ack,
    anthropic: byProvider("anthropic"), openai: byProvider("openai"),
    google: byProvider("google"), ollama: byProvider("ollama"), custom: byProvider("custom"),
  };
}

export default function SettingsPage(_props: PageProps) {
  const [view, setView] = useState<api.LlmSettingsView | null>(null);
  const [selected, setSelected] = useState<api.LlmProvider>("anthropic");
  const [keyInput, setKeyInput] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [test, setTest] = useState<api.TestResult | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api.getLlmSettings()
      .then((v) => { setView(v); if (v.active_provider) setSelected(v.active_provider); })
      .catch((e) => setErr(String(e)));

  useEffect(() => { void load(); }, []);
  if (!view) return null;

  const current = view.providers.find((p) => p.provider === selected)!;
  const patchConfig = (p: Partial<api.ProviderConfig>) =>
    setView({
      ...view,
      providers: view.providers.map((x) =>
        x.provider === selected ? { ...x, config: { ...x.config, ...p } } : x),
    });

  const cloudBlocked = CLOUD[selected] && !view.cloud_egress_ack;

  const save = async () => {
    setErr(null); setMsg(null); setBusy(true);
    try {
      await api.setLlmSettings({ ...toSettings(view), active_provider: selected });
      if (USES_KEY[selected] && keyInput.trim()) {
        await api.setLlmKey(selected, keyInput.trim());
        setKeyInput("");
      }
      await load();
      setMsg("Saved.");
    } catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const runTest = async () => {
    setErr(null); setMsg(null); setTest(null); setBusy(true);
    try { setTest(await api.testLlmConnection(selected)); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const clearKey = async () => {
    setBusy(true);
    try { await api.clearLlmKey(selected); await load(); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div style={{ width: "100%", maxWidth: 720, margin: "0 auto" }}>
      <PageTitle eyebrow="Customer Intelligence" title="Settings" actions={undefined} />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      {msg && <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>{msg}</Alert>}

      <Card>
        <h2 style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-lg)", margin: "0 0 var(--space-4)" }}>
          AI Agent
        </h2>

        <Field label="Provider" hint={undefined} error={undefined} htmlFor={undefined}>
          <Select
            value={selected}
            options={api.PROVIDERS.map((p) => ({ value: p, label: LABEL[p] }))}
            children={undefined}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setSelected(e.target.value as api.LlmProvider); setTest(null); setKeyInput("");
            }}
          />
        </Field>

        <Field label="Model" hint={undefined} error={undefined} htmlFor={undefined}>
          <Input value={current.config.model}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchConfig({ model: e.target.value })} />
        </Field>

        <Field label="Base URL" hint={undefined} error={undefined} htmlFor={undefined}>
          <Input value={current.config.base_url}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchConfig({ base_url: e.target.value })} />
        </Field>

        <Field label="Timeout (seconds)" hint={undefined} error={undefined} htmlFor={undefined}>
          <Input type="number" value={String(current.config.timeout_secs)}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
              patchConfig({ timeout_secs: Number(e.target.value) || 0 })} />
        </Field>

        {USES_KEY[selected] && (
          <Field label="API key" hint={undefined} error={undefined} htmlFor={undefined}>
            {current.has_key
              ? (<div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center" }}>
                  <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>•••• set</span>
                  <Button variant="secondary" size="sm" disabled={busy} onClick={clearKey}>Clear</Button>
                </div>)
              : (<Input type="password" value={keyInput} placeholder="Paste key"
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => setKeyInput(e.target.value)} />)}
          </Field>
        )}

        {CLOUD[selected] && (
          <Alert tone="warning" style={{ margin: "var(--space-4) 0" }}>
            <label style={{ display: "flex", gap: "var(--space-2)", alignItems: "flex-start" }}>
              <input type="checkbox" checked={view.cloud_egress_ack}
                onChange={(e) => setView({ ...view, cloud_egress_ack: e.target.checked })} />
              <span>I understand this provider sends congregation data to an external service.</span>
            </label>
          </Alert>
        )}

        <div style={{ display: "flex", gap: "var(--space-3)", marginTop: "var(--space-4)" }}>
          <Button disabled={busy || cloudBlocked} onClick={save}>Save</Button>
          <Button variant="secondary" disabled={busy || cloudBlocked} onClick={runTest}>Test connection</Button>
        </div>

        {test && (
          <Alert tone={test.ok ? "success" : "error"} style={{ marginTop: "var(--space-4)" }}>
            {test.ok ? "Connection OK" : "Connection failed"} — {test.detail}
          </Alert>
        )}
      </Card>
    </div>
  );
}
