import { useCallback, useEffect, useRef, useState } from "react";
import type { ChangeEvent, KeyboardEvent } from "react";
import { Card, IconButton, Textarea, Menu, Select, Button, Icon, Alert } from "./design-system";
import chatLogoUrl from "./assets/emanuel_icon.png";
import * as api from "./api";

/** A rendered chat turn. While a reply streams, the trailing assistant message is `streaming`. */
interface Msg { role: "user" | "assistant"; content: string; streaming?: boolean }

const BACKEND_LABEL: Record<api.ChatBackend, string> = {
  ollama: "Ollama (local)",
  claude: "Claude",
  "chat-gpt": "ChatGPT",
};

function normalizeBackend(b: string): api.ChatBackend {
  return b === "claude" || b === "chat-gpt" ? b : "ollama";
}

/** A short conversation title from the first question. */
function titleFrom(text: string): string {
  const t = text.trim().replace(/\s+/g, " ");
  return t.length <= 48 ? t : `${t.slice(0, 47)}…`;
}

/**
 * The global chat overlay: a bottom-right launcher toggling a panel that asks the governed
 * membership aggregates natural-language questions. Three keyless backends, streaming replies,
 * saved conversations. Mounted once in `App.tsx`, outside the page router, so it is available on
 * every page. The only data any backend receives is the Rust-built governed snapshot; this UI
 * never sees or sends household data.
 */
export default function ChatWidget() {
  const [open, setOpen] = useState(false);
  const [backend, setBackend] = useState<api.ChatBackend>("ollama");
  const [statuses, setStatuses] = useState<api.ChatBackendStatus[]>([]);
  const [conversations, setConversations] = useState<api.ChatConversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renameText, setRenameText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [fabHover, setFabHover] = useState(false);

  // The event handlers run outside React's render, so they read the active conversation from a ref
  // to avoid a stale closure routing tokens to the wrong (or a closed) conversation.
  const activeIdRef = useRef<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  const setActive = useCallback((id: string | null) => {
    activeIdRef.current = id;
    setActiveId(id);
  }, []);

  const reloadConversations = useCallback(async () => {
    try { setConversations(await api.chatListConversations()); } catch { /* non-fatal */ }
  }, []);

  const refreshStatus = useCallback(async () => {
    try { setStatuses(await api.chatBackendStatus()); } catch { /* non-fatal */ }
  }, []);

  // Subscribe once to the streaming events; route each to the active conversation via the ref.
  useEffect(() => {
    const unlisten: Array<Promise<() => void>> = [];
    unlisten.push(api.onChatToken((p) => {
      if (p.conversation_id !== activeIdRef.current) return;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant" && last.streaming) {
          return [...prev.slice(0, -1), { ...last, content: last.content + p.token }];
        }
        return [...prev, { role: "assistant", content: p.token, streaming: true }];
      });
    }));
    unlisten.push(api.onChatDone((p) => {
      if (p.conversation_id !== activeIdRef.current) return;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant" && last.streaming) {
          return [...prev.slice(0, -1), { role: "assistant", content: p.content || last.content }];
        }
        return [...prev, { role: "assistant", content: p.content }];
      });
      setStreaming(false);
      void reloadConversations();
    }));
    unlisten.push(api.onChatError((p) => {
      if (p.conversation_id !== activeIdRef.current) return;
      setError(p.error);
      setStreaming(false);
      setMessages((prev) => (prev[prev.length - 1]?.streaming ? prev.slice(0, -1) : prev));
    }));
    return () => { unlisten.forEach((u) => void u.then((f) => f())); };
  }, [reloadConversations]);

  // Load availability and saved conversations when the panel first opens.
  useEffect(() => {
    if (open) { void refreshStatus(); void reloadConversations(); }
  }, [open, refreshStatus, reloadConversations]);

  // Keep the newest message in view as it streams.
  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const currentStatus = statuses.find((s) => s.backend === backend);
  const unavailable = currentStatus ? !currentStatus.available : false;

  const newChat = useCallback(() => {
    setActive(null);
    setMessages([]);
    setError(null);
    setRenaming(false);
    setDrawerOpen(false);
  }, [setActive]);

  const openConversation = useCallback(async (c: api.ChatConversation) => {
    setActive(c.id);
    setBackend(normalizeBackend(c.backend));
    setError(null);
    setDrawerOpen(false);
    setRenaming(false);
    try {
      const msgs = await api.chatListMessages(c.id);
      setMessages(msgs.map((m) => ({ role: m.role === "assistant" ? "assistant" : "user", content: m.content })));
    } catch (e) { setError(String(e)); }
  }, [setActive]);

  const send = useCallback(async () => {
    const text = input.trim();
    if (!text || streaming || unavailable) return;
    let convId = activeIdRef.current;
    if (!convId) {
      try {
        const c = await api.chatCreateConversation(backend, titleFrom(text));
        convId = c.id;
        setActive(c.id);
      } catch (e) { setError(String(e)); return; }
    }
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    setInput("");
    setError(null);
    setStreaming(true);
    try { await api.chatSend(convId, backend, text); }
    catch (e) { setError(String(e)); setStreaming(false); }
    void reloadConversations();
  }, [input, streaming, unavailable, backend, setActive, reloadConversations]);

  const cancel = useCallback(async () => {
    const id = activeIdRef.current;
    if (!id) return;
    try { await api.chatCancel(id); } catch { /* best effort */ }
    setStreaming(false);
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      return last?.streaming ? [...prev.slice(0, -1), { role: "assistant", content: last.content }] : prev;
    });
  }, []);

  const remove = useCallback(async () => {
    const id = activeIdRef.current;
    if (!id) return;
    try { await api.chatDeleteConversation(id); } catch (e) { setError(String(e)); return; }
    newChat();
    void reloadConversations();
  }, [newChat, reloadConversations]);

  const commitRename = useCallback(async () => {
    const id = activeIdRef.current;
    const title = renameText.trim();
    setRenaming(false);
    if (!id || !title) return;
    try { await api.chatRenameConversation(id, title); await reloadConversations(); }
    catch (e) { setError(String(e)); }
  }, [renameText, reloadConversations]);

  const clearHistory = useCallback(async () => {
    try { await api.chatClearHistory(); } catch (e) { setError(String(e)); return; }
    setConversations([]);
    newChat();
  }, [newChat]);

  // ── closed: the launcher ───────────────────────────────────────────────────
  if (!open) {
    return (
      <div style={{ position: "fixed", right: "var(--space-6)", bottom: "var(--space-6)", zIndex: "var(--z-dropdown, 1000)" }}>
        <button type="button" aria-label="Open chat" onClick={() => setOpen(true)}
          onMouseEnter={() => setFabHover(true)} onMouseLeave={() => setFabHover(false)}
          style={{
            display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: 66, height: 66, padding: 0,
            borderRadius: 20,
            background: "transparent",
            border: "none",
            cursor: "pointer",
            // The app-tile icon is its own surface; a brand-tinted glow ring plus a deep shadow
            // make the launcher read as a distinct, inviting affordance over any page, and it
            // lifts and brightens on hover.
            boxShadow: fabHover
              ? "0 0 0 4px var(--color-primary-100, #e5eefb), var(--shadow-2xl)"
              : "0 0 0 3px var(--color-primary-100, #e5eefb), var(--shadow-xl)",
            transform: fabHover ? "translateY(-3px) scale(1.05)" : "none",
            transition: "var(--transition-all)",
          }}>
          <img src={chatLogoUrl} alt="" style={{ width: 66, height: 66, borderRadius: 20, objectFit: "cover", display: "block", pointerEvents: "none" }} />
        </button>
      </div>
    );
  }

  const activeConv = conversations.find((c) => c.id === activeId) ?? null;
  const menuItems = [
    { key: "new", label: "New chat", icon: "plus", onSelect: newChat },
    { key: "saved", label: "Saved conversations", icon: "list", onSelect: () => setDrawerOpen((v) => !v) },
    { key: "rename", label: "Rename", icon: "pencil", disabled: !activeConv, onSelect: () => { setRenameText(activeConv?.title ?? ""); setRenaming(true); } },
    { key: "delete", label: "Delete", icon: "trash-2", disabled: !activeId, onSelect: remove },
    { divider: true },
    { key: "clear", label: "Clear chat history", icon: "eraser", onSelect: clearHistory },
  ];

  // ── open: the panel ────────────────────────────────────────────────────────
  return (
    <div role="dialog" aria-label="Data chat" style={{ position: "fixed", right: "var(--space-6)", bottom: "var(--space-6)", zIndex: "var(--z-dropdown, 1000)" }}>
      <Card padded={false} style={{ width: 380, height: 560, display: "flex", flexDirection: "column", overflow: "hidden", boxShadow: "var(--shadow-2xl)" }}>
        {/* Header */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", padding: "var(--space-3) var(--space-4)", borderBottom: "1px solid var(--border-subtle)" }}>
          {renaming ? (
            <input aria-label="Conversation title" autoFocus value={renameText}
              onChange={(e) => setRenameText(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") void commitRename(); if (e.key === "Escape") setRenaming(false); }}
              onBlur={() => void commitRename()}
              style={{ flex: 1, minWidth: 0, fontFamily: "var(--font-display)", fontSize: "var(--text-sm)", padding: "var(--space-1) var(--space-2)", border: "1px solid var(--border-default)", borderRadius: "var(--radius-md)" }} />
          ) : (
            <div style={{ flex: 1, minWidth: 0, fontFamily: "var(--font-display)", fontSize: "var(--text-base)", fontWeight: "var(--font-semibold)", color: "var(--text-primary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {activeConv?.title ?? "Ask the data"}
            </div>
          )}
          <Menu align="right" trigger={({ toggle }: { open: boolean; toggle: () => void }) => (
            <IconButton aria-label="Conversation menu" onClick={toggle}><Icon name="ellipsis-vertical" size={18} /></IconButton>
          )} items={menuItems} />
          <IconButton aria-label="Close chat" onClick={() => setOpen(false)}><Icon name="x" size={18} /></IconButton>
        </div>

        {/* Backend selector */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", padding: "var(--space-2) var(--space-4)", borderBottom: "1px solid var(--border-subtle)" }}>
          <span style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>Model</span>
          <Select size="sm" aria-label="Backend" value={backend} style={{ flex: 1 }}
            children={undefined}
            options={api.CHAT_BACKENDS.map((b) => {
              const st = statuses.find((s) => s.backend === b.key);
              const suffix = st ? (st.available ? "" : " — unavailable") : "";
              return { value: b.key, label: `${BACKEND_LABEL[b.key]}${suffix}` };
            })}
            onChange={(e: ChangeEvent<HTMLSelectElement>) => setBackend(e.target.value as api.ChatBackend)} />
        </div>

        {/* Body: saved-conversations drawer OR message list */}
        {drawerOpen ? (
          <div style={{ flex: 1, overflowY: "auto", padding: "var(--space-2)" }}>
            {conversations.length === 0 ? (
              <div style={{ padding: "var(--space-6) var(--space-4)", textAlign: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>No saved conversations yet.</div>
            ) : conversations.map((c) => (
              <button key={c.id} onClick={() => void openConversation(c)}
                style={{ display: "flex", justifyContent: "space-between", alignItems: "center", width: "100%", gap: "var(--space-2)", padding: "var(--space-2) var(--space-3)", border: "none", background: c.id === activeId ? "var(--bg-secondary)" : "transparent", borderRadius: "var(--radius-md)", cursor: "pointer", textAlign: "left" }}>
                <span style={{ minWidth: 0, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>{c.title}</span>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary, var(--text-secondary))" }}>{BACKEND_LABEL[normalizeBackend(c.backend)]}</span>
              </button>
            ))}
          </div>
        ) : (
          <div ref={listRef} style={{ flex: 1, overflowY: "auto", padding: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            {messages.length === 0 && (
              <div style={{ margin: "auto", textAlign: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)", maxWidth: 280 }}>
                Ask a question about the membership data — for example, “Which cohort is most profitable?” Answers use only de-identified aggregates.
              </div>
            )}
            {messages.map((m, i) => (
              <div key={i} style={{ alignSelf: m.role === "user" ? "flex-end" : "flex-start", maxWidth: "85%",
                padding: "var(--space-2) var(--space-3)", borderRadius: "var(--radius-lg)", whiteSpace: "pre-wrap", wordBreak: "break-word",
                fontSize: "var(--text-sm)", lineHeight: "var(--leading-normal)",
                background: m.role === "user" ? "var(--color-primary-500)" : "var(--bg-secondary)",
                color: m.role === "user" ? "var(--text-inverse)" : "var(--text-primary)" }}
                data-role={m.role}>
                {m.content}{m.streaming && <span aria-hidden> ▍</span>}
              </div>
            ))}
          </div>
        )}

        {/* Unavailable notice */}
        {unavailable && (
          <div style={{ padding: "0 var(--space-4) var(--space-2)" }}>
            <Alert tone="warning">{currentStatus?.detail || `${BACKEND_LABEL[backend]} is unavailable.`}</Alert>
          </div>
        )}
        {error && (
          <div style={{ padding: "0 var(--space-4) var(--space-2)" }}>
            <Alert tone="error">{error}</Alert>
          </div>
        )}

        {/* Composer */}
        <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "flex-end", padding: "var(--space-3) var(--space-4)", borderTop: "1px solid var(--border-subtle)" }}>
          <Textarea rows={2} aria-label="Message" placeholder={unavailable ? "This backend is unavailable" : "Ask about the data…"}
            value={input} disabled={unavailable}
            onChange={(e: ChangeEvent<HTMLTextAreaElement>) => setInput(e.target.value)}
            onKeyDown={(e: KeyboardEvent<HTMLTextAreaElement>) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); void send(); } }}
            style={{ minHeight: 0, resize: "none" }} />
          {streaming ? (
            <Button variant="secondary" onClick={() => void cancel()} aria-label="Stop">
              <Icon name="square" size={14} /> Stop
            </Button>
          ) : (
            <Button onClick={() => void send()} disabled={unavailable || !input.trim()} aria-label="Send">
              <Icon name="send" size={14} /> Send
            </Button>
          )}
        </div>
      </Card>
    </div>
  );
}
