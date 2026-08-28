// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup, act } from "@testing-library/react";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const listeners = vi.hoisted(() => new Map<string, Set<(e: { payload: unknown }) => void>>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: (e: { payload: unknown }) => void) => {
    if (!listeners.has(name)) listeners.set(name, new Set());
    listeners.get(name)!.add(cb);
    return Promise.resolve(() => { listeners.get(name)?.delete(cb); });
  }),
}));
function emit(name: string, payload: unknown) {
  act(() => { listeners.get(name)?.forEach((cb) => cb({ payload })); });
}

import ChatWidget from "./ChatWidget";

// Route invoke by command name. Tests override pieces as needed.
function defaultRoutes(): Record<string, unknown> {
  return {
    chat_backend_status: [
      { backend: "ollama", available: true, detail: "Local Ollama server reachable" },
      { backend: "claude", available: false, detail: "`claude` not found on PATH" },
      { backend: "chat-gpt", available: false, detail: "`codex` not found on PATH" },
    ],
    chat_list_conversations: [],
    chat_list_messages: [],
    chat_create_conversation: { id: "conv-1", backend: "ollama", title: "Which cohort?", session_id: null, created_at: "", updated_at: "" },
    chat_send: undefined,
    chat_cancel: undefined,
    chat_clear_history: undefined,
  };
}

beforeEach(() => {
  invoke.mockReset();
  listeners.clear();
  const routes = defaultRoutes();
  invoke.mockImplementation((cmd: string) => Promise.resolve(routes[cmd]));
});
afterEach(() => cleanup());

const open = () => fireEvent.click(screen.getByLabelText("Open chat"));

describe("ChatWidget", () => {
  it("launcher opens and closes the panel", async () => {
    render(<ChatWidget />);
    expect(screen.queryByRole("dialog")).toBeNull();
    open();
    expect(screen.getByRole("dialog", { name: "Data chat" })).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Close chat"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByLabelText("Open chat")).toBeTruthy();
  });

  it("selecting an unavailable backend shows the unavailable state and blocks sending", async () => {
    render(<ChatWidget />);
    open();
    // Wait for backend statuses to load.
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_backend_status"));
    const select = screen.getByLabelText("Backend") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "claude" } });
    // The unavailable detail is shown and the composer is disabled — nothing is sent.
    await waitFor(() => expect(screen.getByText(/not found on PATH/)).toBeTruthy());
    expect((screen.getByLabelText("Message") as HTMLTextAreaElement).disabled).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("chat_send", expect.anything());
  });

  it("streams an assistant reply token by token and finalizes on done", async () => {
    render(<ChatWidget />);
    open();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_backend_status"));

    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "Which cohort is most profitable?" } });
    fireEvent.click(screen.getByLabelText("Send"));

    // A conversation is created, then the turn is sent.
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_create_conversation", { backend: "ollama", title: "Which cohort is most profitable?" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_send", { conversationId: "conv-1", backend: "ollama", message: "Which cohort is most profitable?" }));

    // The user's message is shown, then streamed tokens accumulate.
    expect(screen.getByText("Which cohort is most profitable?")).toBeTruthy();
    emit("chat:token", { conversation_id: "conv-1", token: "The FY2015 " });
    emit("chat:token", { conversation_id: "conv-1", token: "cohort." });
    await waitFor(() => expect(screen.getByText(/The FY2015 cohort\./)).toBeTruthy());

    emit("chat:done", { conversation_id: "conv-1", message_id: "m1", content: "The FY2015 cohort." });
    // After done, the composer returns to the idle Send state (no Stop button).
    await waitFor(() => expect(screen.getByLabelText("Send")).toBeTruthy());
    expect(screen.queryByLabelText("Stop")).toBeNull();
  });

  it("cancels an in-progress reply", async () => {
    render(<ChatWidget />);
    open();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_backend_status"));
    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "hi" } });
    fireEvent.click(screen.getByLabelText("Send"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_send", expect.anything()));

    // While streaming, the Stop control is offered and cancels the run.
    const stop = await screen.findByLabelText("Stop");
    fireEvent.click(stop);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_cancel", { conversationId: "conv-1" }));
    await waitFor(() => expect(screen.getByLabelText("Send")).toBeTruthy());
  });

  it("routes tokens only to the active conversation", async () => {
    render(<ChatWidget />);
    open();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_backend_status"));
    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "hi" } });
    fireEvent.click(screen.getByLabelText("Send"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("chat_send", expect.anything()));

    // A token for a different conversation must not render here.
    emit("chat:token", { conversation_id: "some-other", token: "LEAK" });
    expect(screen.queryByText(/LEAK/)).toBeNull();
  });
});
