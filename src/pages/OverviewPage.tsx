import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, CardHeader, CardTitle, Modal } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";

type Step = "scan" | "select" | "sync" | "profile" | "ready";

function nextStep(s: api.StatusView): Step {
  if (s.object_count === 0) return "scan";
  if (s.selected_count === 0) return "select";
  if (s.synced_rows === 0) return "sync";
  return "ready";
}

const STEP_COPY: Record<Step, { title: string; body: string; button: string | null }> = {
  scan: { title: "Scan your org", body: "Reads object and field names only. No records are copied.", button: "Scan metadata" },
  select: { title: "Choose objects to mirror", body: "Nothing is copied until you select objects on the Data page.", button: null },
  sync: { title: "Sync selected objects", body: "Copies the selected objects into the encrypted local mirror, minus withheld fields.", button: "Sync now" },
  profile: { title: "Profile columns", body: "Compute fill rates and top values so you can see which fields carry signal.", button: "Profile" },
  ready: { title: "Data is ready", body: "Re-sync any time to refresh the mirror. Profiling runs automatically after each sync.", button: "Sync again" },
};

export default function OverviewPage({ status, refresh }: PageProps) {
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<string>("");
  const [notice, setNotice] = useState<{ tone: "success" | "warning" | "error"; text: string } | null>(null);
  const [confirmPurge, setConfirmPurge] = useState(false);

  useEffect(() => {
    const subs = [
      api.onScanProgress((p) => setProgress(`Scanning ${p.done} of ${p.total} objects`)),
      api.onSyncProgress((p) => setProgress(`${p.object}: ${p.rows.toLocaleString()} rows`)),
    ];
    return () => { subs.forEach((s) => s.then((un) => un())); };
  }, []);

  const run = async (label: string, fn: () => Promise<string>) => {
    setBusy(label); setNotice(null); setProgress("");
    try { setNotice({ tone: "success", text: await fn() }); }
    catch (e) { setNotice({ tone: "error", text: String(e) }); }
    finally { setBusy(null); setProgress(""); await refresh(); }
  };

  const doScan = () => run("scan", async () => {
    const r = await api.scan();
    return `Scanned ${r.objects} objects.${r.failed.length ? ` ${r.failed.length} could not be described.` : ""}`;
  });
  const doSync = () => run("sync", async () => {
    const r = await api.syncSelected();
    const n = await api.profileSelected();
    return `Synced ${r.rows.toLocaleString()} rows across ${r.objects_synced} objects; profiled ${n}.${r.failed.length ? ` Failed: ${r.failed.join("; ")}` : ""}`;
  });

  const step = nextStep(status);
  const copy = STEP_COPY[step];
  const onPrimary = step === "scan" ? doScan : doSync;

  return (
    <div style={{ maxWidth: 1100 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Overview" actions={
        <Button variant="secondary" disabled={busy !== null} onClick={doScan}>Rescan metadata</Button>
      } />

      {notice && <Alert tone={notice.tone} style={{ marginBottom: "var(--space-6)" }}>{notice.text}</Alert>}

      <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "var(--space-4)", marginBottom: "var(--space-6)" }}>
        <Stat label="Objects scanned" value={status.object_count.toLocaleString()} icon="database"
          sub={status.last_scan_at ? `Last scan ${new Date(status.last_scan_at).toLocaleString()}` : "Not scanned yet"} />
        <Stat label="Objects selected" value={status.selected_count.toLocaleString()} icon="square-check" tone="accent" sub={undefined} />
        <Stat label="Rows mirrored" value={status.synced_rows.toLocaleString()} icon="hard-drive" tone="success" sub={undefined} />
        <Stat label="Connected as" value={status.identity?.display_name ?? "—"} icon="user" tone="neutral"
          sub={status.identity?.username} />
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "2fr 1fr", gap: "var(--space-4)" }}>
        <Card>
          <CardHeader><CardTitle>{copy.title}</CardTitle></CardHeader>
          <p style={{ color: "var(--text-secondary)", marginTop: 0 }}>{copy.body}</p>
          {progress && <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)", marginBottom: "var(--space-3)" }}>{progress}</div>}
          {copy.button && <Button disabled={busy !== null} onClick={onPrimary}>{busy ? "Working…" : copy.button}</Button>}
        </Card>

        <Card>
          <CardHeader><CardTitle>Session</CardTitle></CardHeader>
          <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", marginTop: 0 }}>
            Tokens and the mirror's encryption key are held in Windows Credential Manager. Disconnecting revokes the session but keeps local data.
          </p>
          <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
            <Button variant="secondary" disabled={busy !== null} onClick={() => run("disconnect", async () => { await api.disconnect(); return "Disconnected."; })}>Disconnect</Button>
            <Button variant="danger" disabled={busy !== null || status.synced_rows === 0} onClick={() => setConfirmPurge(true)}>Purge local data</Button>
          </div>
        </Card>
      </div>

      <Modal open={confirmPurge} onClose={() => setConfirmPurge(false)} title="Purge local data?" size="sm"
        footer={<>
          <Button variant="secondary" onClick={() => setConfirmPurge(false)}>Cancel</Button>
          <Button variant="danger" onClick={() => { setConfirmPurge(false); void run("purge", async () => { await api.purgeLocalData(); return "Local mirror deleted. Catalog and audit log kept."; }); }}>Purge</Button>
        </>}>
        <p style={{ margin: 0, color: "var(--text-secondary)" }}>
          This deletes every mirrored row and profile from this computer. Your object selections and the audit log are kept. This action is recorded.
        </p>
      </Modal>
    </div>
  );
}
