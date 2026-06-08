import {
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Markdown from "react-markdown";
import {
  Sparkles,
  X,
  Send,
  Wrench,
  Loader2,
  ChevronRight,
  ChevronDown,
  ShieldAlert,
} from "lucide-react";
import { errorText } from "../lib/api";
import { Modal } from "./Modal";
import type { AnalyzeTarget } from "./TerminalDock";

type ToolItem = {
  kind: "tool";
  id: string;
  name: string;
  detail: string;
  result?: string;
  isError?: boolean;
};
type ChatItem =
  | { kind: "user"; text: string }
  | { kind: "assistant"; text: string }
  | ToolItem
  | { kind: "note"; text: string }
  | { kind: "error"; text: string };

/** A command claude was denied and is asking the user to approve. */
type Approval = { command: string; patterns: string[]; compound: boolean };

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `chat-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

/** Flatten a tool_result `content` (string, or an array of `{text}` blocks). */
function toolResultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return (content as Array<{ text?: unknown }>)
      .map((b) => (b && typeof b.text === "string" ? b.text : ""))
      .join("");
  }
  return "";
}

/** Derive `--allowedTools` patterns from a denied tool call, scoped to the command
 * family the user approved. Compound commands (`git fetch … && git merge …`) yield one
 * pattern per sub-command so the whole thing is covered on retry. E.g.
 * `gh pr merge 7 --squash` → [`Bash(gh pr merge:*)`];
 * `git fetch origin x && git merge x` → [`Bash(git fetch:*)`, `Bash(git merge:*)`]. */
function toolPatterns(d: {
  tool_name?: string;
  tool_input?: { command?: string };
}): string[] {
  const name = d.tool_name ?? "Bash";
  const cmd = d.tool_input?.command;
  if (name !== "Bash" || typeof cmd !== "string" || !cmd.trim()) return [name];
  const pats = new Set<string>();
  for (const part of cmd.split(/&&|\|\||;|\|/)) {
    const tokens = part.trim().split(/\s+/).filter(Boolean);
    if (!tokens.length) continue;
    const max = tokens[0] === "gh" ? 3 : 2; // `gh pr merge` needs 3; `git merge` needs 2
    const prefix: string[] = [];
    for (const t of tokens) {
      if (t.startsWith("-") || /\d/.test(t) || prefix.length >= max) break;
      prefix.push(t);
    }
    if (prefix.length) pats.add(`Bash(${prefix.join(" ")}:*)`);
  }
  return pats.size ? [...pats] : [`Bash(${cmd.trim().split(/\s+/)[0]}:*)`];
}

/** Shell metacharacters that chain or segment a command. Approving a compound command must
 * never auto-whitelist a tacked-on second verb (`gh pr merge 7 && rm -rf ~`), so we detect
 * these and require the model to run sub-commands one at a time instead. */
const COMPOUND_RE = /&&|\|\||;|\||\n|`|\$\(/;
function isCompound(cmd: string): boolean {
  return COMPOUND_RE.test(cmd);
}

function ToolChip({ item }: { item: ToolItem }) {
  const [open, setOpen] = useState(false);
  const hasResult = item.result != null;
  return (
    <div className="text-[11px]">
      <button
        onClick={() => hasResult && setOpen((o) => !o)}
        disabled={!hasResult}
        className="flex w-full items-center gap-1.5 text-left text-neutral-500 enabled:hover:text-neutral-300"
      >
        <Wrench className="h-3 w-3 shrink-0" />
        <span className="font-medium text-neutral-400">{item.name}</span>
        {item.detail && (
          <span className="truncate font-mono text-neutral-600">{item.detail}</span>
        )}
        {hasResult &&
          (open ? (
            <ChevronDown className="ml-auto h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="ml-auto h-3 w-3 shrink-0" />
          ))}
      </button>
      {open && item.result && (
        <pre
          className={`mt-1 max-h-48 overflow-auto whitespace-pre-wrap rounded border px-2 py-1 font-mono text-[10px] ${
            item.isError
              ? "border-red-900 bg-red-950/40 text-red-300"
              : "border-neutral-800 bg-neutral-950 text-neutral-400"
          }`}
        >
          {item.result}
        </pre>
      )}
    </div>
  );
}

/** A conversational alternative to TerminalDock: drives the headless `chat_*` commands
 * and renders `claude --output-format stream-json` events as chat bubbles. In merge mode
 * the write command is denied and surfaced as an approval modal. */
export function ChatDock({
  repoPath,
  target,
  mode,
  streaming,
  aiName,
  onClose,
}: {
  repoPath: string;
  target: AnalyzeTarget;
  mode: string;
  /** Render answers progressively (typewriter) vs once complete. */
  streaming: boolean;
  /** Display name of the active AI engine (Ollama model, or "Claude"). */
  aiName: string;
  onClose: () => void;
}) {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [running, setRunning] = useState(true);
  const [input, setInput] = useState("");
  const [height, setHeight] = useState(380);
  const [approval, setApproval] = useState<Approval | null>(null);

  const idRef = useRef<string | null>(null);
  if (idRef.current === null) idRef.current = newId();
  const sessionRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Whether a streaming assistant bubble is currently open (for delta accumulation).
  const liveRef = useRef(false);

  const mergeMode =
    target.kind === "merge-branches" || (target.kind === "pr" && mode === "merge");

  const targetKey =
    target.kind === "commit"
      ? target.sha
      : target.kind === "pr"
      ? `pr-${target.number}-${mode}`
      : target.kind === "merge-branches"
      ? `merge-${target.source}-${target.target}`
      : "repo";

  // Append a streamed text chunk to the open assistant bubble (or open a new one).
  function appendDelta(chunk: string) {
    const open = liveRef.current;
    liveRef.current = true;
    setItems((it) => {
      const last = it[it.length - 1];
      if (open && last && last.kind === "assistant") {
        const copy = it.slice();
        copy[copy.length - 1] = { kind: "assistant", text: last.text + chunk };
        return copy;
      }
      return [...it, { kind: "assistant", text: chunk }];
    });
  }

  // Parse one stream-json event line and fold it into the chat.
  function handleEvent(line: string) {
    let ev: {
      type?: string;
      session_id?: string;
      message?: { content?: unknown[] };
      event?: { type?: string; delta?: { type?: string; text?: string } };
      permission_denials?: Array<{
        tool_name?: string;
        tool_input?: { command?: string; file_path?: string };
      }>;
    };
    try {
      ev = JSON.parse(line);
    } catch {
      return;
    }
    if (!sessionRef.current && typeof ev.session_id === "string") {
      sessionRef.current = ev.session_id;
    }

    // Streaming mode: text arrives progressively as content_block_delta chunks.
    if (streaming && ev.type === "stream_event" && ev.event) {
      const inner = ev.event;
      if (inner.type === "content_block_delta" && inner.delta?.type === "text_delta") {
        const chunk = inner.delta.text;
        if (typeof chunk === "string" && chunk.length) appendDelta(chunk);
      } else if (inner.type === "content_block_stop" || inner.type === "message_stop") {
        liveRef.current = false; // close the current streaming bubble
      }
      return;
    }

    if (ev.type === "assistant" && Array.isArray(ev.message?.content)) {
      for (const block of ev.message!.content as Array<Record<string, unknown>>) {
        if (block.type === "tool_use") {
          const input = (block.input ?? {}) as Record<string, unknown>;
          const detail =
            typeof input.command === "string"
              ? input.command
              : typeof input.description === "string"
              ? input.description
              : typeof input.file_path === "string"
              ? input.file_path
              : "";
          const id = String(block.id ?? `${Date.now()}-${Math.random()}`);
          setItems((it) => [
            ...it,
            { kind: "tool", id, name: String(block.name ?? "tool"), detail },
          ]);
        } else if (
          block.type === "text" &&
          typeof block.text === "string" &&
          block.text.trim()
        ) {
          // In streaming mode the text already arrived via deltas — don't duplicate it.
          if (!streaming) {
            const text = block.text as string;
            setItems((it) => [...it, { kind: "assistant", text }]);
          }
        }
        // `thinking` blocks are intentionally not rendered.
      }
      return;
    }

    // Tool results come back as `user` messages — attach to the matching chip.
    if (ev.type === "user" && Array.isArray(ev.message?.content)) {
      for (const block of ev.message!.content as Array<Record<string, unknown>>) {
        if (block.type === "tool_result" && typeof block.tool_use_id === "string") {
          const result = toolResultText(block.content);
          const isError = block.is_error === true;
          const tid = block.tool_use_id as string;
          setItems((it) =>
            it.map((item) =>
              item.kind === "tool" && item.id === tid
                ? { ...item, result, isError }
                : item
            )
          );
        }
      }
      return;
    }

    // End of turn: in merge mode, a denied write becomes an approval request.
    if (ev.type === "result") {
      const denials = ev.permission_denials;
      if (mergeMode && Array.isArray(denials) && denials.length > 0) {
        const d = denials[0];
        const command = String(
          d.tool_input?.command ?? d.tool_input?.file_path ?? d.tool_name ?? ""
        );
        const compound =
          typeof d.tool_input?.command === "string" && isCompound(d.tool_input.command);
        setApproval({ command, patterns: toolPatterns(d), compound });
      }
      return;
    }
  }

  // (Re)start a session whenever the target/mode/streaming changes.
  useEffect(() => {
    const id = idRef.current!;
    let alive = true;
    sessionRef.current = null;
    liveRef.current = false;
    setItems([]);
    setInput("");
    setApproval(null);
    setRunning(true);
    const unlisteners: Array<() => void> = [];

    (async () => {
      const offEvent = await listen<{ id: string; line: string }>("chat-event", (e) => {
        if (e.payload.id === id) handleEvent(e.payload.line);
      });
      const offEnd = await listen<{ id: string; ok: boolean; stderr: string }>(
        "chat-turn-end",
        (e) => {
          if (e.payload.id !== id) return;
          setRunning(false);
          // Surface a startup failure (e.g. auth) when nothing else came back.
          if (e.payload.stderr?.trim() && !sessionRef.current) {
            const text = e.payload.stderr.trim();
            setItems((it) => [...it, { kind: "error", text }]);
          }
        }
      );
      unlisteners.push(offEvent, offEnd);
      if (!alive) {
        offEvent();
        offEnd();
        return;
      }

      let cmd: string;
      let args: Record<string, unknown> | null;
      if (target.kind === "commit") {
        cmd = "chat_open_analyze";
        args = { id, path: repoPath, sha: target.sha, mode, partial: streaming, extraAllowed: [] };
      } else if (target.kind === "merge-branches") {
        cmd = "chat_open_merge_branches";
        args = {
          id,
          path: repoPath,
          source: target.source,
          target: target.target,
          partial: streaming,
          extraAllowed: [],
        };
      } else if (target.kind === "pr" && mode === "merge") {
        cmd = "chat_open_merge_pr";
        args = {
          id,
          path: repoPath,
          number: target.number,
          partial: streaming,
          extraAllowed: [],
        };
      } else if (target.kind === "pr") {
        cmd = "chat_open_analyze_pr";
        args = { id, path: repoPath, number: target.number, mode, partial: streaming, extraAllowed: [] };
      } else if (target.kind === "repo") {
        cmd = "chat_open_repo";
        args = { id, path: repoPath, partial: streaming, extraAllowed: [] };
      } else {
        cmd = "";
        args = null;
      }
      if (!args) return;
      await invoke(cmd, args).catch((err) => {
        setItems((it) => [...it, { kind: "error", text: errorText(err) }]);
        setRunning(false);
      });
    })();

    return () => {
      alive = false;
      unlisteners.forEach((u) => u());
      invoke("chat_close", { id }).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, targetKey, mode, streaming]);

  // Keep the latest message in view.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [items, running, approval]);

  // `extra` is the one-shot allowlist for THIS turn only (a just-approved command). It is
  // never persisted, so an approval can't silently widen permissions for later turns.
  function sendTurn(text: string, extra: string[] = []) {
    if (!sessionRef.current) return;
    setRunning(true);
    invoke("chat_send", {
      id: idRef.current,
      path: repoPath,
      sessionId: sessionRef.current,
      text,
      partial: streaming,
      extraAllowed: extra,
    }).catch((err) => {
      setItems((it) => [...it, { kind: "error", text: errorText(err) }]);
      setRunning(false);
    });
  }

  function send() {
    const text = input.trim();
    if (!text || running || !sessionRef.current) return;
    setItems((it) => [...it, { kind: "user", text }]);
    setInput("");
    sendTurn(text);
  }

  function approve() {
    if (!approval) return;
    const { command, patterns, compound } = approval;
    setApproval(null);
    setItems((it) => [...it, { kind: "note", text: `✓ Autorisé : ${command}` }]);
    if (compound) {
      // Don't whitelist several sub-commands from one click. Ask the model to run them one
      // at a time; each individual write then prompts for its own approval.
      sendTurn(
        "J'autorise cette opération, mais exécute les commandes UNE PAR UNE — sans `&&`, " +
          "`;`, `|` ni enchaînement. Lance seulement la première commande maintenant.",
        []
      );
      return;
    }
    // Single-turn scope: the allowance is handed to this retry only, never persisted.
    sendTurn("J'autorise cette commande. Exécute-la maintenant.", patterns);
  }

  function refuse() {
    if (!approval) return;
    const { command } = approval;
    setApproval(null);
    setItems((it) => [...it, { kind: "note", text: `✗ Refusé : ${command}` }]);
    sendTurn("Je refuse cette commande. Ne l'exécute pas ; propose une alternative ou arrête-toi.");
  }

  function onKeyDown(e: ReactKeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function startResize(e: ReactMouseEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const startH = height;
    const onMove = (ev: MouseEvent) =>
      setHeight(
        Math.min(window.innerHeight * 0.85, Math.max(200, startH - (ev.clientY - startY)))
      );
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.userSelect = "";
    };
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  const label =
    target.kind === "commit"
      ? `commit ${target.sha.slice(0, 8)} · ${mode}`
      : target.kind === "pr"
      ? `PR #${target.number} · ${mode}`
      : target.kind === "merge-branches"
      ? `merge ${target.source} → ${target.target}`
      : "dépôt";

  const canSend = !running && !!sessionRef.current && !!input.trim();

  return (
    <div
      style={{ height }}
      className="flex shrink-0 flex-col border-t border-neutral-800 bg-neutral-950"
    >
      <div
        onMouseDown={startResize}
        className="h-1 shrink-0 cursor-row-resize bg-neutral-800 transition-colors hover:bg-indigo-600"
      />
      <div className="flex h-9 shrink-0 items-center gap-2 px-3 text-xs text-neutral-300">
        <Sparkles className="h-3.5 w-3.5 shrink-0 text-indigo-400" />
        <span className="font-medium" title={aiName}>
          {aiName}
        </span>
        <span className="truncate font-mono text-neutral-500">{label}</span>
        {mergeMode && (
          <span className="rounded bg-emerald-900/40 px-1.5 py-0.5 text-[10px] text-emerald-300">
            merge
          </span>
        )}
        <button
          onClick={onClose}
          className="ml-auto rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div ref={scrollRef} className="min-h-0 flex-1 space-y-3 overflow-auto px-3 py-2">
        {items.length === 0 && running && (
          <div className="flex items-center gap-2 text-xs text-neutral-500">
            <Loader2 className="h-3.5 w-3.5 animate-spin" /> {aiName} analyse…
          </div>
        )}
        {items.map((it, i) => {
          if (it.kind === "user") {
            return (
              <div key={i} className="flex justify-end">
                <div className="max-w-[85%] whitespace-pre-wrap rounded-2xl rounded-br-sm bg-indigo-600 px-3 py-1.5 text-sm text-white">
                  {it.text}
                </div>
              </div>
            );
          }
          if (it.kind === "assistant") {
            return (
              <div key={i} className="flex justify-start">
                <div className="max-w-[85%] space-y-2 rounded-2xl rounded-bl-sm border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm leading-relaxed text-neutral-200 [&_a]:text-indigo-400 [&_code]:rounded [&_code]:bg-neutral-800 [&_code]:px-1 [&_ol]:list-decimal [&_ol]:pl-5 [&_pre]:overflow-auto [&_pre]:rounded [&_pre]:bg-neutral-950 [&_pre]:p-2 [&_ul]:list-disc [&_ul]:pl-5">
                  <Markdown>{it.text}</Markdown>
                </div>
              </div>
            );
          }
          if (it.kind === "tool") {
            return <ToolChip key={i} item={it} />;
          }
          if (it.kind === "note") {
            return (
              <div key={i} className="text-center text-[11px] text-neutral-500">
                {it.text}
              </div>
            );
          }
          return (
            <div
              key={i}
              className="rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-xs text-red-300"
            >
              {it.text}
            </div>
          );
        })}
        {items.length > 0 && running && (
          <div className="flex items-center gap-2 text-xs text-neutral-500">
            <Loader2 className="h-3.5 w-3.5 animate-spin" /> {aiName} réfléchit…
          </div>
        )}
      </div>

      <div className="flex shrink-0 items-end gap-2 border-t border-neutral-800 p-2">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          rows={1}
          placeholder={
            sessionRef.current ? `Pose une question à ${aiName}…` : "Démarrage de la session…"
          }
          disabled={running && !sessionRef.current}
          className="max-h-32 min-h-[2.25rem] flex-1 resize-none rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-indigo-600 disabled:opacity-50"
        />
        <button
          onClick={send}
          disabled={!canSend}
          title="Envoyer (Entrée)"
          className="inline-flex h-9 items-center gap-1.5 rounded-md bg-indigo-600 px-3 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-40"
        >
          <Send className="h-4 w-4" />
        </button>
      </div>

      {approval && (
        <Modal title="Autorisation requise" onClose={refuse}>
          <div className="space-y-3">
            <div className="flex items-start gap-2 text-sm text-neutral-300">
              <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
              <span>{aiName} veut exécuter cette commande :</span>
            </div>
            <pre className="overflow-auto rounded border border-neutral-700 bg-neutral-950 p-2 font-mono text-xs text-amber-300">
              {approval.command}
            </pre>
            {approval.compound && (
              <div className="flex items-start gap-2 rounded border border-amber-900 bg-amber-950/40 px-2 py-1.5 text-[11px] text-amber-300">
                <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  Commande composée (enchaînement détecté). Si tu autorises, {aiName} sera
                  invité à exécuter les commandes une par une — chacune devra être autorisée
                  séparément.
                </span>
              </div>
            )}
            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={refuse}
                className="rounded-md px-3 py-1.5 text-sm text-neutral-400 hover:bg-neutral-800"
              >
                Refuser
              </button>
              <button
                onClick={approve}
                className="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500"
              >
                Autoriser
              </button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
