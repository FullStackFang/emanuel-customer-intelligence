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
});
