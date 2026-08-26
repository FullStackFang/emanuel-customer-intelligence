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

export interface Kpis {
  members_now: number; net_vs_prior_fy: number; joins_this_fy: number; resigns_this_fy: number;
  year1_cohort: number; year1_pct: number; year1_baseline_pct: number; at_risk_count: number;
}
export interface TrendRow { fy: number; joins: number; resigns: number; active_end_of_fy: number }
export interface CohortYear1 { cohort: number; n: number; pct_retained: number }
export interface CohortCell { cohort: number; n: number; k: number; pct_retained: number }
export interface ChannelRow { key: string; label: string; n: number; still_members: number; pct: number; avg_tenure: number; left_within_2y: number }
export interface SchoolRow { group: string; n: number; still_members: number; pct: number }
export interface ReasonCell { fy: number; reason: string; n: number }
export interface MultiJobRow { bucket: string; jobs: number; n: number; still_members: number; pct: number; avg_tenure: number }
export interface OutcomeByTenureRow { tenure_bucket: string; outcome: string; n: number }
export interface SchoolGapRow { bucket: string; n: number; still_members: number; pct: number }
export interface DuesRow { fy: number; active: number; billed: number; coverage_missing: number; settled: number; partially_settled: number; unsettled: number }
export interface AnchorTypeRow { key: string; label: string; n: number; still_members: number; pct: number }
export interface AnchorCountRow { anchors: number; label: string; n: number; still_members: number; pct: number }
export interface AtRiskRow { account_id: string; name: string; tier: string | null; join_fy: number | null; rules: string[] }
export interface SourceCapability {
  key: string; available: boolean; required_objects: string[];
  mirrored_columns: string[];
  last_synced_at: string | null; unavailable_reason: string | null;
}
export interface Insights {
  built_at: string | null; newest_source_sync_at: string | null; stale: boolean;
  capabilities: SourceCapability[]; current_fy: number; unavailable: string[]; kpis: Kpis;
  trend: TrendRow[]; year1: CohortYear1[]; cohort_matrix: CohortCell[];
  channels: ChannelRow[]; school: SchoolRow[]; reasons: ReasonCell[];
  multi_job: MultiJobRow[]; outcome_by_tenure: OutcomeByTenureRow[];
  school_progression: SchoolRow[]; school_gap: SchoolGapRow[];
  dues: DuesRow[]; anchor_type: AnchorTypeRow[]; anchor_count: AnchorCountRow[];
}
export const INSIGHT_VIEWS = [
  "trend", "year1", "cohort_matrix", "channels", "school", "reasons", "at_risk",
  "multi_job", "outcome_by_tenure", "school_progression", "school_gap",
  "dues", "anchor_type", "anchor_count",
] as const;
export type InsightView = (typeof INSIGHT_VIEWS)[number];

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

export const getInsights = (forceRebuild = false) => invoke<Insights>("get_insights", { forceRebuild });
export const getAtRisk = () => invoke<AtRiskRow[]>("get_at_risk");
export const exportInsightsCsv = (view: InsightView) => invoke<string>("export_insights_csv", { view });
export const revealExport = (path: string) => invoke<void>("reveal_export", { path });
export const exportInsightsPdf = () => invoke<string>("export_insights_pdf");

// ── Validated membership risk ────────────────────────────────────────────────
export interface YearSummary { test_fy: number; households: number; exits: number; sufficient: boolean }
export interface FamilyCoverageView { family: string; train: number; score: number; kept: boolean }
export interface RiskSummary {
  available: boolean; unavailable_reason: string | null;
  roc_auc: number; top_decile_lift: number; brier: number; baseline_brier: number;
  years: YearSummary[]; coverage: FamilyCoverageView[]; removed_families: string[];
  model_first_fy: number | null; model_last_fy: number | null; watch_list_count: number;
}
export interface EvidenceView { class: string; detail: string }
export interface WatchRowView { account_id: string; name: string; score: number; evidence: EvidenceView[] }
export interface WatchListView {
  available: boolean; unavailable_reason: string | null;
  model_first_fy: number | null; model_last_fy: number | null;
  baseline_rate: number; confidence: number; rows: WatchRowView[];
}
export const getRiskSummary = () => invoke<RiskSummary>("get_risk_summary");
export const getWatchList = () => invoke<WatchListView>("get_watch_list");
export const exportWatchListCsv = () => invoke<string>("export_watch_list_csv");

export const onScanProgress = (cb: (p: { done: number; total: number }) => void): Promise<UnlistenFn> =>
  listen<{ done: number; total: number }>("scan:progress", (e) => cb(e.payload));
export const onSyncProgress = (cb: (p: { object: string; rows: number }) => void): Promise<UnlistenFn> =>
  listen<{ object: string; rows: number }>("sync:progress", (e) => cb(e.payload));

export type LlmProvider = "anthropic" | "openai" | "google" | "ollama" | "custom";
export const PROVIDERS: LlmProvider[] = ["anthropic", "openai", "google", "ollama", "custom"];

export interface ProviderConfig {
  model: string; base_url: string; timeout_secs: number; headers: Record<string, string>;
}
export interface ProviderView { provider: LlmProvider; config: ProviderConfig; has_key: boolean }
export interface LlmSettingsView {
  active_provider: LlmProvider | null; cloud_egress_ack: boolean; providers: ProviderView[];
}
export interface LlmSettings {
  active_provider: LlmProvider | null; cloud_egress_ack: boolean;
  anthropic: ProviderConfig; openai: ProviderConfig; google: ProviderConfig;
  ollama: ProviderConfig; custom: ProviderConfig;
}
export interface TestResult { ok: boolean; detail: string }

export const getLlmSettings = () => invoke<LlmSettingsView>("get_llm_settings");
export const setLlmSettings = (settings: LlmSettings) => invoke<void>("set_llm_settings", { settings });
export const setLlmKey = (provider: LlmProvider, key: string) =>
  invoke<void>("set_llm_key", { provider, key });
export const clearLlmKey = (provider: LlmProvider) => invoke<void>("clear_llm_key", { provider });
export const testLlmConnection = (provider: LlmProvider) =>
  invoke<TestResult>("test_llm_connection", { provider });
