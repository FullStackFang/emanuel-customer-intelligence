import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// The ONLY way the UI talks to Salesforce or the local mirror. 1:1 with Rust
// commands. No token, no SQL, no network ever crosses into this layer.

export interface Identity { user_id: string; organization_id: string; username: string; display_name: string }
export interface StatusView {
  connected: boolean; identity: Identity | null;
  object_count: number; selected_count: number; synced_rows: number; last_scan_at: string | null;
}
export interface ObjectRow {
  name: string; label: string; record_count: number; selected: boolean;
  last_synced_at: string | null; last_sync_rows: number | null;
}
export interface FieldRow {
  field: string; sf_type: string; label: string; sensitive: boolean; withheld: boolean;
  fill_rate: number | null; distinct_count: number | null; top_values: string | null;
}
export interface ScanSummary { objects: number; failed: string[] }
export interface SyncSummary { objects_synced: number; rows: number; failed: string[] }
export interface Filter { field: string; op: string; value: string }
export interface SegmentReq { object: string; filters: Filter[]; group_by?: string }
export interface SegmentResult { count: number; breakdown: [string, number][] }
export interface AuditRow {
  id: number; at: string; sf_user_id: string | null; sf_username: string | null;
  action: string; object: string | null; detail: string | null;
}

export const OPS = ["=", "!=", ">", "<", ">=", "<=", "contains"] as const;

export const getStatus = () => invoke<StatusView>("get_status");
export const connect = () => invoke<Identity>("connect");
export const disconnect = () => invoke<void>("disconnect");
export const scan = () => invoke<ScanSummary>("scan");
export const listObjects = () => invoke<ObjectRow[]>("list_objects");
export const setObjectSelected = (object: string, selected: boolean) =>
  invoke<void>("set_object_selected", { object, selected });
export const listFields = (object: string) => invoke<FieldRow[]>("list_fields", { object });
export const setFieldWithheld = (object: string, field: string, withheld: boolean) =>
  invoke<void>("set_field_withheld", { object, field, withheld });
export const syncSelected = () => invoke<SyncSummary>("sync_selected");
export const profileSelected = () => invoke<number>("profile_selected");
export const querySegment = (req: SegmentReq) => invoke<SegmentResult>("query_segment", { req });
export const getAudit = (limit: number, offset: number) => invoke<AuditRow[]>("get_audit", { limit, offset });
export const purgeLocalData = () => invoke<void>("purge_local_data");

export const onScanProgress = (cb: (p: { done: number; total: number }) => void): Promise<UnlistenFn> =>
  listen<{ done: number; total: number }>("scan:progress", (e) => cb(e.payload));
export const onSyncProgress = (cb: (p: { object: string; rows: number }) => void): Promise<UnlistenFn> =>
  listen<{ object: string; rows: number }>("sync:progress", (e) => cb(e.payload));
