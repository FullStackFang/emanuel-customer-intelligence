import { useCallback, useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, Field, Input } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

/** How each backend authenticates and how to sign in — all keyless. */
const BACKEND_META: Record<api.ChatBackend, { label: string; auth: string }> = {
  ollama: { label: "Ollama (local)", auth: "Runs entirely on this machine — nothing leaves it." },
  claude: { label: "Claude", auth: "Uses your Claude Code CLI login. Run `claude` in a terminal to sign in." },
  "chat-gpt": { label: "ChatGPT", auth: "Uses your Codex CLI login. Run `codex login` in a terminal to sign in." },
};

function StatusPill({ available }: { available: boolean }) {
  const [bg, fg, label] = available
    ? ["var(--color-success-50)", "var(--color-success-700)", "Available"]
    : ["var(--bg-secondary)", "var(--text-secondary)", "Not available"];
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: "var(--space-2)",
      padding: "2px var(--space-2)", borderRadius: "var(--radius-full, 999px)",
      background: bg, color: fg, fontSize: "var(--text-xs)", fontWeight: "var(--font-medium)", whiteSpace: "nowrap",
    }}>
      <span style={{ width: 7, height: 7, borderRadius: "50%", background: available ? "var(--color-success-500)" : "var(--border-strong)" }} />
      {label}
    </span>
  );
}

// Rebuild the full LlmSettings the backend expects from the array-shaped view. Only the local
// Ollama config is user-editable here; the other providers' stored config is preserved untouched.
function toSettings(view: api.LlmSettingsView): api.LlmSettings {
  const byProvider = (p: api.LlmProvider) => view.providers.find((x) => x.provider === p)!.config;
  return {
    // The chat never uses the API-key path, so the "active provider" is pinned to the local
    // Ollama backend — which also keeps `set_llm_settings` clear of the cloud-egress gate.
    active_provider: "ollama",
    cloud_egress_ack: view.cloud_egress_ack,
    anthropic: byProvider("anthropic"), openai: byProvider("openai"),
    google: byProvider("google"), ollama: byProvider("ollama"), custom: byProvider("custom"),
  };
}

export default function SettingsPage(_props: PageProps) {
  const [view, setView] = useState<api.LlmSettingsView | null>(null);
  const [statuses, setStatuses] = useState<api.ChatBackendStatus[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() =>
    api.getLlmSettings()
      .then((v) => setView(v))
      .catch((e) => setErr(String(e))), []);

  const refreshStatus = useCallback(() =>
    api.chatBackendStatus()
      .then(setStatuses)
      .catch((e) => setErr(String(e))), []);

  useEffect(() => { void load(); void refreshStatus(); }, [load, refreshStatus]);
  if (!view) return null;

  const ollama = view.providers.find((p) => p.provider === "ollama")!.config;
  const patchOllama = (p: Partial<api.ProviderConfig>) =>
    setView({
      ...view,
      providers: view.providers.map((x) =>
        x.provider === "ollama" ? { ...x, config: { ...x.config, ...p } } : x),
    });
  const statusOf = (b: api.ChatBackend) => statuses.find((s) => s.backend === b);

  const saveOllama = async () => {
    setErr(null); setMsg(null); setBusy(true);
    try {
      await api.setLlmSettings(toSettings(view));
      await load();
      await refreshStatus();
      setMsg("Saved.");
    } catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const checkAgain = async () => {
    setErr(null); setMsg(null); setBusy(true);
    try { await refreshStatus(); } finally { setBusy(false); }
  };

  return (
    <div style={{ width: "100%", maxWidth: 720, margin: "0 auto" }}>
      <PageTitle eyebrow="Customer Intelligence" title="Settings" actions={undefined} />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      {msg && <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>{msg}</Alert>}

      <Card>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-3)", marginBottom: "var(--space-2)" }}>
          <h2 style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-lg)", margin: 0 }}>AI Chat</h2>
          <Button variant="secondary" size="sm" disabled={busy} onClick={checkAgain}>Check again</Button>
        </div>
        <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", lineHeight: "var(--leading-normal)", margin: "0 0 var(--space-5)" }}>
          The assistant answers questions about the membership data with no API keys. It uses your
          computer's own <strong>Claude</strong> and <strong>ChatGPT</strong> subscriptions (through the
          Claude&nbsp;Code and Codex command-line tools) or a local <strong>Ollama</strong> server. Only a
          de-identified aggregate snapshot is ever sent — never a household's name, address, or record.
        </p>

        {/* Backend availability */}
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginBottom: "var(--space-5)" }}>
          {api.CHAT_BACKENDS.map(({ key }) => {
            const st = statusOf(key);
            return (
              <div key={key} style={{
                display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)",
                padding: "var(--space-3)", borderRadius: "var(--radius-lg)", border: "1px solid var(--border-subtle)",
              }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: "var(--text-sm)", fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>
                    {BACKEND_META[key].label}
                  </div>
                  <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2 }}>
                    {BACKEND_META[key].auth}
                  </div>
                  {st?.detail && (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary, var(--text-secondary))", marginTop: 4, fontFamily: "var(--font-mono, monospace)" }}>
                      {st.detail}
                    </div>
                  )}
                </div>
                <StatusPill available={st?.available ?? false} />
              </div>
            );
          })}
        </div>

        {/* Keyless local Ollama configuration */}
        <div style={{ borderTop: "1px solid var(--border-subtle)", paddingTop: "var(--space-4)" }}>
          <h3 style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-base)", margin: "0 0 var(--space-1)" }}>
            Local Ollama
          </h3>
          <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)", margin: "0 0 var(--space-3)" }}>
            The address and model of your local Ollama server. Claude and ChatGPT need no settings here —
            they use their CLI login.
          </p>
          <Field label="Server URL" hint={undefined} error={undefined} htmlFor={undefined}>
            <Input value={ollama.base_url}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchOllama({ base_url: e.target.value })} />
          </Field>
          <Field label="Model" hint={undefined} error={undefined} htmlFor={undefined}>
            <Input value={ollama.model} placeholder="e.g. llama3.1"
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchOllama({ model: e.target.value })} />
          </Field>
          <div style={{ marginTop: "var(--space-4)" }}>
            <Button disabled={busy} onClick={saveOllama}>Save</Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
