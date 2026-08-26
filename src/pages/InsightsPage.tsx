import type React from "react";
import { useCallback, useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, CardHeader, CardTitle, EmptyState, Select } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";
import { CohortHeatmap, FlowsChart, HBarChart, ReasonsChart, TableView, TrendChart, Year1Chart } from "./insights/charts";
import { RULE_LABELS, fmt, fyLabel, soWhat } from "./insights/format";

function SoWhat({ text }: { text: string }) {
  return (
    <p style={{ margin: "var(--space-3) 0 0", padding: "var(--space-2) var(--space-3)", background: "var(--bg-secondary)", borderRadius: "var(--radius-md)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
      <span style={{ fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>So what: </span>{text}
    </p>
  );
}

function Lede({ children }: { children: string }) {
  return <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{children}</p>;
}

function Unavailable({ column }: { column: string }) {
  return <EmptyState icon="database" title="Not available" message={`This view needs ${column} to be synced and not withheld.`} action={undefined} />;
}

export default function InsightsPage({ status }: PageProps) {
  const [ins, setIns] = useState<api.Insights | null>(null);
  const [atRisk, setAtRisk] = useState<api.AtRiskRow[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [exportView, setExportView] = useState<api.InsightView>("trend");
  const [exported, setExported] = useState<string | null>(null);

  const load = useCallback(async (force = false) => {
    setBusy(force ? "rebuild" : "load"); setErr(null);
    try { setIns(await api.getInsights(force)); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(null); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const showAtRisk = async () => {
    setBusy("risk"); setErr(null);
    try { setAtRisk(await api.getAtRisk()); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };
  const doExport = async () => {
    setBusy("export"); setErr(null); setExported(null);
    try { setExported(await api.exportInsightsCsv(exportView)); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };

  if (status.synced_rows === 0) {
    return (
      <div>
        <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={undefined} />
        <EmptyState icon="chart-line" title="Nothing synced yet" message="Select Account on the Data page and run Sync now from the Overview page. Insights are built from the local mirror after each sync." action={undefined} />
      </div>
    );
  }

  const missing = (col: string) => ins?.unavailable.includes(col) ?? false;
  const s = ins ? soWhat(ins) : null;
  const latestTwo = ins ? ins.year1.slice(-2).map((r) => r.cohort) : [];
  const built = ins?.built_at ? new Date(ins.built_at).toLocaleString() : "not built";

  return (
    <div style={{ maxWidth: 1180 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={
        <>
          <Select value={exportView} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setExportView(e.target.value as api.InsightView)}
            options={api.INSIGHT_VIEWS.map((v) => ({ value: v, label: v.replace("_", " ") }))} children={undefined} />
          <Button variant="secondary" disabled={busy !== null || !ins} onClick={() => void doExport()}>Export CSV</Button>
          <Button variant="secondary" disabled={busy !== null} onClick={() => void load(true)}>{busy === "rebuild" ? "Rebuilding…" : "Rebuild"}</Button>
        </>
      } />
      <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)", marginTop: "calc(-1 * var(--space-4))", marginBottom: "var(--space-4)" }}>
        Built {built} · fiscal years run June 1 – May 31 and are labeled by the year they end
      </div>
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      {exported && (
        <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>
          Exported to <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{exported}</span>{" "}
          <Button size="sm" variant="secondary" onClick={() => void api.revealExport(exported)}>Reveal</Button>
        </Alert>
      )}

      {!ins ? <EmptyState icon="loader" title="Loading insights" message="Reading the local mirror." action={undefined} /> : (
        <>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-5)" }}>
            <Stat label="Member households" value={fmt(ins.kpis.members_now)} sub={`${ins.kpis.net_vs_prior_fy >= 0 ? "+" : ""}${fmt(ins.kpis.net_vs_prior_fy)} vs ${fyLabel(ins.current_fy - 1)}`} icon="users" tone="primary" />
            <Stat label={`Joins ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.joins_this_fy)} sub="fiscal year to date" icon="user-plus" tone="success" />
            <Stat label={`Resignations ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.resigns_this_fy)} sub="fiscal year to date" icon="user-minus" tone="neutral" />
            <Stat label="First-year retention" value={`${ins.kpis.year1_pct}%`} sub={`${fyLabel(ins.kpis.year1_cohort)} cohort · baseline ${ins.kpis.year1_baseline_pct}%`} icon="repeat" tone="primary" />
            <Stat label="Households at risk" value={fmt(ins.kpis.at_risk_count)} sub="current members matching a churn pattern" icon="triangle-alert" tone="accent" />
          </div>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Membership over time</CardTitle></CardHeader>
            <Lede>Active member households at the end of each fiscal year, and the joins and resignations behind them.</Lede>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-4)" }}>
              <TrendChart rows={ins.trend} />
              <FlowsChart rows={ins.trend} />
            </div>
            <SoWhat text={s!.trend} />
            <TableView rows={ins.trend} getRowKey={(r) => String(r.fy)} columns={[
              { key: "fy", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
              { key: "j", header: "Joins", align: "right", render: (r) => fmt(r.joins) },
              { key: "r", header: "Resignations", align: "right", render: (r) => fmt(r.resigns) },
              { key: "a", header: "Active at year end", align: "right", render: (r) => fmt(r.active_end_of_fy) },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>First-year retention by cohort</CardTitle></CardHeader>
            <Lede>Of the households that joined in each fiscal year, the share still members one year later. The two newest cohorts are highlighted.</Lede>
            <Year1Chart rows={ins.year1} emphasize={latestTwo} />
            <SoWhat text={s!.year1} />
            <TableView rows={ins.year1} getRowKey={(r) => String(r.cohort)} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
              { key: "p", header: "Still members after 1 year", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Cohort retention</CardTitle></CardHeader>
            <Lede>Each row is a join-year cohort; each cell is the share still members after that many years. Blank cells are years that haven't happened yet.</Lede>
            <CohortHeatmap cells={ins.cohort_matrix} />
            <SoWhat text={s!.cohort} />
            <TableView rows={ins.cohort_matrix} getRowKey={(r) => `${r.cohort}-${r.k}`} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "k", header: "Years after", align: "right", render: (r) => r.k },
              { key: "p", header: "Still members", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by join reason</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Households that joined 4–12 fiscal years ago and recorded a reason. Share still members today; a household counts under every reason it named. School-driven reasons are highlighted.</Lede>
                <HBarChart rows={ins.channels.map((c) => ({ label: c.label, pct: c.pct, n: c.n, still: c.still_members }))} emphasize={["Religious school", "Nursery school"]} />
                <SoWhat text={s!.channels} />
                <TableView rows={ins.channels} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Join reason", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
                ]} />
              </>
            )}
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by school history</CardTitle></CardHeader>
            <Lede>Same joiner window, grouped by whether the household ever had a child in nursery or religious school.</Lede>
            <HBarChart rows={ins.school.map((g) => ({ label: g.group, pct: g.pct, n: g.n, still: g.still_members }))} />
            <SoWhat text={s!.school} />
            <TableView rows={ins.school} getRowKey={(r) => r.group} columns={[
              { key: "g", header: "School history", render: (r) => r.group },
              { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
              { key: "p", header: "Share still members", align: "right", render: (r) => `${r.pct}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Why people leave</CardTitle></CardHeader>
            {missing("Resign_Reason__c") ? <Unavailable column="Resign_Reason__c" /> : (
              <>
                <Lede>Coded resignation reasons by fiscal year. Reasons outside the six most common fold into "Other".</Lede>
                <ReasonsChart cells={ins.reasons} />
                <SoWhat text={s!.reasons} />
                <TableView rows={ins.reasons} getRowKey={(r) => `${r.fy}-${r.reason}`} columns={[
                  { key: "f", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
                  { key: "r", header: "Reason", render: (r) => r.reason },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                ]} />
              </>
            )}
          </Card>

          <Card>
            <CardHeader><CardTitle>Households at risk</CardTitle></CardHeader>
            <Lede>Current members matching a churn pattern: first year of membership, nursery-school-only joiners, introductory tiers aging out, or families whose religious-school years just ended. Viewing this list is recorded in the audit log.</Lede>
            {atRisk === null
              ? <Button variant="secondary" disabled={busy !== null} onClick={() => void showAtRisk()}>{busy === "risk" ? "Loading…" : `Show ${fmt(ins.kpis.at_risk_count)} households`}</Button>
              : <TableView rows={atRisk} getRowKey={(r) => r.account_id} empty="No households match the at-risk rules." columns={[
                  { key: "n", header: "Household", render: (r) => r.name },
                  { key: "t", header: "Tier", render: (r) => r.tier ?? "—" },
                  { key: "j", header: "Joined", render: (r) => (r.join_fy ? fyLabel(r.join_fy) : "—") },
                  { key: "r", header: "Patterns", render: (r) => <span style={{ display: "inline-flex", gap: 4, flexWrap: "wrap" }}>{r.rules.map((k) => <Badge key={k} tone="warning">{RULE_LABELS[k] ?? k}</Badge>)}</span> },
                ]} />}
          </Card>
        </>
      )}
    </div>
  );
}
