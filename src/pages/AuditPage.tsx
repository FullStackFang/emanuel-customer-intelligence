import type React from "react";
import { useCallback, useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Badge, Button, Card, Table } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

// The design-system Table.jsx destructures `columns = []` / `rows = []` with
// no JSDoc, so allowJs+strict infers both as `never[]` when consumed from a
// .tsx file. Retype it locally for this page's audit-row shape rather than
// editing the design-system file (see DataPage's `FieldTable`).
const AuditTable = Table as unknown as (props: {
  getRowKey: (r: api.AuditRow) => number;
  rows: api.AuditRow[];
  empty: string;
  columns: { key: string; header: string; width?: number; render: (r: api.AuditRow) => React.ReactNode }[];
}) => React.JSX.Element;

const PAGE = 50;
const TONE: Record<string, "primary" | "success" | "warning" | "error" | "info" | "neutral"> = {
  "auth.connect": "success", "auth.disconnect": "neutral", "scan.run": "info",
  "object.select": "primary", "object.deselect": "neutral", "field.override": "warning", "field.rewithhold": "success",
  "sync.object": "success", "sync.object_failed": "error", "profile.run": "info", "segment.query": "primary", "data.purge": "error",
};

export default function AuditPage(_: PageProps) {
  const [rows, setRows] = useState<api.AuditRow[]>([]);
  const [offset, setOffset] = useState(0);
  const load = useCallback(async (o: number) => { setRows(await api.getAudit(PAGE, o)); setOffset(o); }, []);
  useEffect(() => { void load(0); }, [load]);

  return (
    <div>
      <PageTitle eyebrow="Customer Intelligence" title="Audit" actions={
        <>
          <Button variant="secondary" size="sm" disabled={offset === 0} onClick={() => void load(Math.max(0, offset - PAGE))}>Newer</Button>
          <Button variant="secondary" size="sm" disabled={rows.length < PAGE} onClick={() => void load(offset + PAGE)}>Older</Button>
        </>
      } />
      <Card padded={false}>
        <AuditTable
          getRowKey={(r: api.AuditRow) => r.id}
          rows={rows}
          empty="No activity recorded yet."
          columns={[
            { key: "at", header: "When", width: 190, render: (r: api.AuditRow) => <span style={{ fontVariantNumeric: "tabular-nums", fontSize: "var(--text-xs)" }}>{new Date(r.at).toLocaleString()}</span> },
            { key: "who", header: "Who", render: (r: api.AuditRow) => <span style={{ fontSize: "var(--text-xs)" }}>{r.sf_username ?? "—"}</span> },
            { key: "action", header: "Action", render: (r: api.AuditRow) => <Badge tone={TONE[r.action] ?? "neutral"}>{r.action}</Badge> },
            { key: "object", header: "Object", render: (r: api.AuditRow) => <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{r.object ?? ""}</span> },
            { key: "detail", header: "Detail", render: (r: api.AuditRow) => <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-2xs)", color: "var(--text-tertiary)" }}>{r.detail ?? ""}</span> },
          ]}
        />
      </Card>
    </div>
  );
}
