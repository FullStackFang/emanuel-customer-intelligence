import type React from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, EmptyState, Input, Table } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

// The design-system Table.jsx destructures `columns = []` / `rows = []` with
// no JSDoc, so allowJs+strict infers both as `never[]` when consumed from a
// .tsx file. Retype it locally for this page's field-row shape rather than
// editing the design-system file.
const FieldTable = Table as unknown as (props: {
  getRowKey: (r: api.FieldRow) => string;
  rows: api.FieldRow[];
  empty: string;
  columns: { key: string; header: string; align?: "left" | "right" | "center"; render: (r: api.FieldRow) => React.ReactNode }[];
}) => React.JSX.Element;

function FillBar({ rate }: { rate: number | null }) {
  if (rate === null) return <span style={{ color: "var(--text-tertiary)" }}>—</span>;
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)" }}>
      <span style={{ width: 72, height: 6, background: "var(--color-neutral-200)", borderRadius: "var(--radius-full)", overflow: "hidden", display: "inline-block" }}>
        <span style={{ display: "block", height: "100%", width: `${Math.round(rate * 100)}%`, background: "var(--color-success-500)" }} />
      </span>
      <span style={{ fontVariantNumeric: "tabular-nums" }}>{Math.round(rate * 100)}%</span>
    </span>
  );
}

export default function DataPage({ status, refresh }: PageProps) {
  const [objects, setObjects] = useState<api.ObjectRow[]>([]);
  const [fields, setFields] = useState<api.FieldRow[]>([]);
  const [current, setCurrent] = useState<string>("");
  const [search, setSearch] = useState("");
  const [onlyPopulated, setOnlyPopulated] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const loadObjects = useCallback(async () => {
    try { const o = await api.listObjects(); setObjects(o); if (!current && o[0]) setCurrent(o[0].name); }
    catch (e) { setErr(String(e)); }
  }, [current]);
  const loadFields = useCallback(async (name: string) => {
    if (!name) return;
    try { setFields(await api.listFields(name)); } catch (e) { setErr(String(e)); }
  }, []);

  useEffect(() => { void loadObjects(); }, [loadObjects]);
  useEffect(() => { void loadFields(current); }, [current, loadFields]);

  const toggleObject = async (o: api.ObjectRow) => {
    await api.setObjectSelected(o.name, !o.selected);
    await loadObjects(); await refresh();
  };
  const toggleWithheld = async (f: api.FieldRow) => {
    await api.setFieldWithheld(current, f.field, !f.withheld);
    await loadFields(current);
  };

  const visibleObjects = useMemo(() => {
    const q = search.trim().toLowerCase();
    return objects.filter((o) => !q || o.name.toLowerCase().includes(q) || o.label.toLowerCase().includes(q));
  }, [objects, search]);
  const visibleFields = useMemo(
    () => (onlyPopulated ? fields.filter((f) => (f.fill_rate ?? 0) > 0) : fields),
    [fields, onlyPopulated]);
  const currentObj = objects.find((o) => o.name === current);

  if (status.object_count === 0) {
    return (
      <div>
        <PageTitle eyebrow="Customer Intelligence" title="Data" actions={undefined} />
        <EmptyState icon="database" title="Nothing scanned yet" message="Run a metadata scan from the Overview page to list the objects you can mirror." action={undefined} />
      </div>
    );
  }

  return (
    <div>
      <PageTitle eyebrow="Customer Intelligence" title="Data" actions={undefined} />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      <div style={{ display: "grid", gridTemplateColumns: "340px 1fr", gap: "var(--space-4)", alignItems: "start" }}>
        <Card padded={false}>
          <div style={{ padding: "var(--space-3)", borderBottom: "1px solid var(--border-default)" }}>
            <Input placeholder="Search objects" value={search} onChange={(e: React.ChangeEvent<HTMLInputElement>) => setSearch(e.target.value)} />
          </div>
          <div style={{ maxHeight: "calc(100vh - 320px)", overflowY: "auto" }}>
            {visibleObjects.map((o) => (
              <div key={o.name} onClick={() => setCurrent(o.name)}
                style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", padding: "var(--space-2) var(--space-3)", cursor: "pointer",
                  background: o.name === current ? "var(--color-primary-50)" : "transparent", borderBottom: "1px solid var(--color-neutral-100)" }}>
                <input type="checkbox" checked={o.selected} onChange={() => void toggleObject(o)} onClick={(e) => e.stopPropagation()} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{o.name}</div>
                  <div style={{ fontSize: "var(--text-2xs)", color: "var(--text-tertiary)" }}>
                    {o.record_count < 0 ? "count unavailable" : `${o.record_count.toLocaleString()} records`}
                    {o.last_synced_at ? ` · mirrored ${o.last_sync_rows?.toLocaleString()}` : ""}
                  </div>
                </div>
                {o.last_synced_at && <Badge tone="success">synced</Badge>}
              </div>
            ))}
          </div>
        </Card>

        <Card>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--space-4)" }}>
            <div>
              <div style={{ fontFamily: "var(--font-mono)", fontWeight: "var(--font-semibold)" }}>{current}</div>
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
                {currentObj?.label} · {fields.length} fields · {fields.filter((f) => f.withheld).length} withheld
              </div>
            </div>
            <label style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
              <input type="checkbox" checked={onlyPopulated} onChange={(e) => setOnlyPopulated(e.target.checked)} /> Only populated
            </label>
          </div>
          <FieldTable
            getRowKey={(r: api.FieldRow) => r.field}
            rows={visibleFields}
            empty="No fields to show."
            columns={[
              { key: "field", header: "Field", render: (r: api.FieldRow) => (
                <span>
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{r.field}</span>
                  {r.sensitive && <Badge tone={r.withheld ? "error" : "warning"} style={{ marginLeft: "var(--space-2)" }}>{r.withheld ? "withheld" : "sensitive · mirrored"}</Badge>}
                </span>) },
              { key: "type", header: "Type", render: (r: api.FieldRow) => <span style={{ color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>{r.sf_type}</span> },
              { key: "fill", header: "Fill", render: (r: api.FieldRow) => <FillBar rate={r.fill_rate} /> },
              { key: "distinct", header: "Distinct", align: "right", render: (r: api.FieldRow) => r.distinct_count ?? "—" },
              { key: "top", header: "Top values", render: (r: api.FieldRow) => (
                <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)", display: "inline-block", maxWidth: 360, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.top_values ?? "—"}</span>) },
              { key: "gov", header: "", render: (r: api.FieldRow) => r.sensitive ? (
                <Button size="sm" variant="secondary" onClick={() => void toggleWithheld(r)}>{r.withheld ? "Include" : "Withhold"}</Button>) : null },
            ]}
          />
        </Card>
      </div>
    </div>
  );
}
