import { useCallback, useEffect, useState } from "react";
import "./design-system/styles.css";
import { Alert, Button } from "./design-system";
import { AppFrame } from "./design-system/ui-kits/grant-management/chrome.jsx";
import logoUrl from "./assets/emanuel_logo.png";
import * as api from "./api";
import OverviewPage from "./pages/OverviewPage";
import DataPage from "./pages/DataPage";
import SegmentsPage from "./pages/SegmentsPage";
import AuditPage from "./pages/AuditPage";

export type PageKey = "overview" | "data" | "segments" | "audit";
export interface PageProps { status: api.StatusView; refresh: () => Promise<void> }

const NAV = [
  { key: "overview", icon: "layout-dashboard", label: "Overview" },
  { key: "data", icon: "database", label: "Data" },
  { key: "segments", icon: "chart-pie", label: "Segments" },
  { key: "audit", icon: "scroll-text", label: "Audit" },
];

function initials(name: string) {
  return name.split(/\s+/).filter(Boolean).slice(0, 2).map((p) => p[0]?.toUpperCase() ?? "").join("") || "?";
}

function SignedOut({ onConnected, error }: { onConnected: () => Promise<void>; error: string | null }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(error);
  const go = async () => {
    setBusy(true); setErr(null);
    try { await api.connect(); await onConnected(); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <div style={{ minHeight: "100vh", display: "flex", alignItems: "center", justifyContent: "center",
      background: "var(--gradient-brand)", fontFamily: "var(--font-body)", padding: "var(--space-6)" }}>
      <div style={{ background: "var(--bg-primary)", borderRadius: "var(--radius-2xl)", boxShadow: "var(--shadow-2xl)",
        padding: "var(--space-10)", maxWidth: 440, width: "100%", textAlign: "center" }}>
        <img src={logoUrl} alt="Temple Emanu-El" style={{ width: 72, height: 72, marginBottom: "var(--space-4)" }} />
        <h1 style={{ margin: 0, fontFamily: "var(--font-display)", fontSize: "var(--text-2xl)", fontWeight: "var(--font-semibold)",
          letterSpacing: "var(--tracking-tight)", color: "var(--text-primary)" }}>Temple Emanu-El</h1>
        <div style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)", letterSpacing: "0.18em", textTransform: "uppercase",
          fontWeight: "var(--font-medium)", marginBottom: "var(--space-6)" }}>Customer Intelligence</div>
        <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", margin: "0 0 var(--space-6)" }}>
          Sign in with your Salesforce account. The login opens in your browser; this app never sees your password.
        </p>
        {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)", textAlign: "left" }}>{err}</Alert>}
        <Button fullWidth disabled={busy} onClick={go}>{busy ? "Waiting for browser…" : "Connect to Salesforce"}</Button>
      </div>
    </div>
  );
}

export default function App() {
  const [status, setStatus] = useState<api.StatusView | null>(null);
  const [page, setPage] = useState<PageKey>("overview");
  const [fatal, setFatal] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try { setStatus(await api.getStatus()); setFatal(null); }
    catch (e) { setFatal(String(e)); setStatus({ connected: false, identity: null, object_count: 0, selected_count: 0, synced_rows: 0, last_scan_at: null }); }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  if (!status) return null;
  if (!status.connected || !status.identity) return <SignedOut onConnected={refresh} error={fatal} />;

  const user = { initials: initials(status.identity.display_name), name: status.identity.display_name, role: "Salesforce" };
  const props: PageProps = { status, refresh };
  return (
    <AppFrame nav={NAV} active={page} onNav={(k: string) => setPage(k as PageKey)} user={user}>
      {page === "overview" && <OverviewPage {...props} />}
      {page === "data" && <DataPage {...props} />}
      {page === "segments" && <SegmentsPage {...props} />}
      {page === "audit" && <AuditPage {...props} />}
    </AppFrame>
  );
}
