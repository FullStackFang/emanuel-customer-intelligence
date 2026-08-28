import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import * as api from "./api";

describe("api wrappers map 1:1 to Rust commands", () => {
  beforeEach(() => invoke.mockReset());

  it("passes exact command names and args", async () => {
    invoke.mockResolvedValue(undefined);
    await api.getStatus();
    await api.setObjectSelected("Account", true);
    await api.setFieldWithheld("Account", "Notes__c", false);
    await api.getAudit(50, 100);
    await api.querySegment({ object: "Account", filters: [{ field: "Type", op: "=", value: "Member" }] });
    expect(invoke.mock.calls).toEqual([
      ["get_status"],
      ["set_object_selected", { object: "Account", selected: true }],
      ["set_field_withheld", { object: "Account", field: "Notes__c", withheld: false }],
      ["get_audit", { limit: 50, offset: 100 }],
      ["query_segment", { req: { object: "Account", filters: [{ field: "Type", op: "=", value: "Member" }] } }],
    ]);
  });

  it("exposes only the allowlisted operators", () => {
    expect([...api.OPS]).toEqual(["=", "!=", ">", "<", ">=", "<=", "contains"]);
  });

  it("insights wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    await api.getInsights();
    await api.getInsights(true);
    await api.getAtRisk();
    await api.exportInsightsCsv("trend");
    await api.revealExport("C:\\x\\exports\\a.csv");
    await api.exportInsightsPdf();
    expect(invoke.mock.calls).toEqual([
      ["get_insights", { forceRebuild: false }],
      ["get_insights", { forceRebuild: true }],
      ["get_at_risk"],
      ["export_insights_csv", { view: "trend" }],
      ["reveal_export", { path: "C:\\x\\exports\\a.csv" }],
      ["export_insights_pdf"],
    ]);
    expect([...api.INSIGHT_VIEWS]).toEqual([
      "trend", "year1", "cohort_matrix", "channels", "school", "reasons", "at_risk",
      "multi_job", "outcome_by_tenure", "school_progression", "school_gap",
      "dues", "anchor_type", "anchor_count",
    ]);
  });

  it("risk wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    await api.getRiskSummary();
    await api.getWatchList();
    await api.exportWatchListCsv();
    expect(invoke.mock.calls).toEqual([
      ["get_risk_summary"],
      ["get_watch_list"],
      ["export_watch_list_csv"],
    ]);
  });

  it("llm settings wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    const cfg = { model: "m", base_url: "u", timeout_secs: 60, headers: {} };
    const settings = {
      active_provider: "anthropic" as const, cloud_egress_ack: true,
      anthropic: cfg, openai: cfg, google: cfg, ollama: cfg, custom: cfg,
    };
    await api.getLlmSettings();
    await api.setLlmSettings(settings);
    await api.setLlmKey("openai", "sk-x");
    await api.clearLlmKey("openai");
    await api.testLlmConnection("ollama");
    expect(invoke.mock.calls).toEqual([
      ["get_llm_settings"],
      ["set_llm_settings", { settings }],
      ["set_llm_key", { provider: "openai", key: "sk-x" }],
      ["clear_llm_key", { provider: "openai" }],
      ["test_llm_connection", { provider: "ollama" }],
    ]);
    expect([...api.PROVIDERS]).toEqual(["anthropic", "openai", "google", "ollama", "custom"]);
  });

  it("chat wrappers use the exact command names and camelCase args", async () => {
    invoke.mockResolvedValue(undefined);
    await api.chatCreateConversation("ollama", "My chat");
    await api.chatListConversations();
    await api.chatListMessages("conv-1");
    await api.chatRenameConversation("conv-1", "Renamed");
    await api.chatDeleteConversation("conv-1");
    await api.chatClearHistory();
    await api.chatBackendStatus();
    await api.chatSend("conv-1", "chat-gpt", "hello");
    await api.chatCancel("conv-1");
    expect(invoke.mock.calls).toEqual([
      ["chat_create_conversation", { backend: "ollama", title: "My chat" }],
      ["chat_list_conversations"],
      ["chat_list_messages", { conversationId: "conv-1" }],
      ["chat_rename_conversation", { conversationId: "conv-1", title: "Renamed" }],
      ["chat_delete_conversation", { conversationId: "conv-1" }],
      ["chat_clear_history"],
      ["chat_backend_status"],
      ["chat_send", { conversationId: "conv-1", backend: "chat-gpt", message: "hello" }],
      ["chat_cancel", { conversationId: "conv-1" }],
    ]);
    expect(api.CHAT_BACKENDS.map((b) => b.key)).toEqual(["ollama", "claude", "chat-gpt"]);
  });
});
