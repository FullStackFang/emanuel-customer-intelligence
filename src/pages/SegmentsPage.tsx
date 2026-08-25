import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, EmptyState, Icon, Input, Select } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

export default function SegmentsPage({ status }: PageProps) {
  const [objects, setObjects] = useState<string[]>([]);
  const [object, setObject] = useState("");
  const [fields, setFields] = useState<api.FieldRow[]>([]);
  const [filters, setFilters] = useState<api.Filter[]>([]);
  const [groupBy, setGroupBy] = useState("");
  const [result, setResult] = useState<api.SegmentResult | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    api.listObjects().then((o) => {
      const synced = o.filter((x) => x.last_synced_at).map((x) => x.name);
      setObjects(synced); if (!object && synced[0]) setObject(synced[0]);
    }).catch((e) => setErr(String(e)));
  }, [object]);
  useEffect(() => {
    if (!object) return;
    api.listFields(object).then((f) => setFields(f.filter((x) => !x.withheld && (x.fill_rate ?? 0) > 0))).catch((e) => setErr(String(e)));
  }, [object]);

  const names = fields.map((f) => f.field);
  const fieldOptions = names.map((n) => ({ value: n, label: n }));
  const add = () => setFilters([...filters, { field: names[0] ?? "", op: "=", value: "" }]);
  const patch = (i: number, p: Partial<api.Filter>) => setFilters(filters.map((f, j) => (j === i ? { ...f, ...p } : f)));
  const remove = (i: number) => setFilters(filters.filter((_, j) => j !== i));

  const run = async () => {
    setErr(null);
    try { setResult(await api.querySegment({ object, filters, group_by: groupBy || undefined })); }
    catch (e) { setErr(String(e)); setResult(null); }
  };

  if (status.synced_rows === 0) {
    return (<div><PageTitle eyebrow="Customer Intelligence" title="Segments" actions={undefined} />
      <EmptyState icon="chart-pie" title="No mirrored data" message="Select objects on the Data page and sync them before building segments." action={undefined} /></div>);
  }
  const max = result?.breakdown.reduce((m, [, n]) => Math.max(m, n), 0) || 1;

  return (
    <div style={{ maxWidth: 1000 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Segments" actions={undefined} />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      <Card style={{ marginBottom: "var(--space-4)" }}>
        <div style={{ display: "grid", gridTemplateColumns: "200px 1fr", gap: "var(--space-3)", alignItems: "center", marginBottom: "var(--space-4)" }}>
          <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>Base object</span>
          <Select value={object} options={objects.map((o) => ({ value: o, label: o }))} children={undefined}
            style={{ fontFamily: "var(--font-mono)" }}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => { setObject(e.target.value); setFilters([]); setGroupBy(""); setResult(null); }} />
        </div>
        {filters.map((f, i) => (
          <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 140px 1fr 40px", gap: "var(--space-2)", marginBottom: "var(--space-2)" }}>
            <Select value={f.field} options={fieldOptions} children={undefined} style={{ fontFamily: "var(--font-mono)" }} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => patch(i, { field: e.target.value })} />
            <Select value={f.op} options={api.OPS.map((o) => ({ value: o, label: o }))} children={undefined} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => patch(i, { op: e.target.value })} />
            <Input value={f.value} placeholder="Value" onChange={(e: React.ChangeEvent<HTMLInputElement>) => patch(i, { value: e.target.value })} />
            <Button variant="secondary" size="sm" onClick={() => remove(i)}><Icon name="x" size={14} /></Button>
          </div>
        ))}
        <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center", marginTop: "var(--space-3)" }}>
          <Button variant="secondary" size="sm" onClick={add}>Add filter</Button>
          <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>Group by</span>
          <div style={{ width: 260 }}>
            <Select value={groupBy} options={[{ value: "", label: "(none)" }, ...fieldOptions]} children={undefined} style={{ fontFamily: "var(--font-mono)" }} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setGroupBy(e.target.value)} />
          </div>
          <div style={{ flex: 1 }} />
          <Button onClick={run}>Run</Button>
        </div>
      </Card>

      {result && (
        <Card>
          <div style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-3xl)", fontWeight: "var(--font-semibold)", letterSpacing: "var(--tracking-tight)" }}>
            {result.count.toLocaleString()} <span style={{ fontSize: "var(--text-sm)", color: "var(--text-tertiary)", fontWeight: "var(--font-normal)" }}>records match</span>
          </div>
          {result.breakdown.length > 0 && (
            <div style={{ marginTop: "var(--space-4)" }}>
              {result.breakdown.map(([label, n]) => (
                <div key={label} style={{ display: "grid", gridTemplateColumns: "200px 1fr 60px", alignItems: "center", gap: "var(--space-3)", marginBottom: "var(--space-1)" }}>
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label || "(blank)"}</div>
                  <div style={{ height: 14, borderRadius: "var(--radius-sm)", background: "var(--color-primary-600)", width: `${Math.max(1, (n / max) * 100)}%` }} />
                  <div style={{ fontSize: "var(--text-sm)", textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{n.toLocaleString()}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
