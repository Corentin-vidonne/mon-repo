import { Fragment, useMemo, useState } from "react";
import { ChevronRight, ChevronDown } from "lucide-react";

/** A changed file for the tree (status optional — PRs don't expose it). */
export type ChangedFile = { path: string; status?: string };
export type DiffViewMode = "unified" | "split";
/** One AI-review finding, annotated inline in the diff. */
export type DiffFinding = {
  file: string;
  line: number | null;
  /** info | warning | critical */
  severity: string;
  title: string;
  detail: string;
};

/** Color for a file's git status letter (A/D/R/M…). */
export function statusColor(s?: string): string {
  if (!s) return "text-neutral-500";
  if (s.startsWith("A")) return "text-emerald-400";
  if (s.startsWith("D")) return "text-red-400";
  if (s.startsWith("R")) return "text-blue-400";
  return "text-amber-400";
}

// ---- view-mode persistence + toggle ----

const DIFF_VIEW_KEY = "gitui.diffView";
export function loadDiffViewMode(): DiffViewMode {
  try {
    return localStorage.getItem(DIFF_VIEW_KEY) === "split" ? "split" : "unified";
  } catch {
    return "unified";
  }
}
export function saveDiffViewMode(v: DiffViewMode) {
  try {
    localStorage.setItem(DIFF_VIEW_KEY, v);
  } catch {
    /* ignore */
  }
}

export function DiffViewToggle({
  value,
  onChange,
}: {
  value: DiffViewMode;
  onChange: (v: DiffViewMode) => void;
}) {
  return (
    <div className="flex shrink-0 rounded-md border border-neutral-700 p-0.5 text-[11px]">
      {(["unified", "split"] as const).map((m) => (
        <button
          key={m}
          onClick={() => onChange(m)}
          className={`rounded px-2 py-0.5 ${
            value === m
              ? "bg-neutral-700 text-neutral-100"
              : "text-neutral-400 hover:text-neutral-200"
          }`}
        >
          {m === "unified" ? "Unifié" : "Côte à côte"}
        </button>
      ))}
    </div>
  );
}

// ---- severity helpers + inline annotation card ----

const SEV_RANK: Record<string, number> = { critical: 3, warning: 2, info: 1 };
function worstSeverity(fs: DiffFinding[]): string {
  return fs.reduce(
    (w, f) => ((SEV_RANK[f.severity] ?? 0) > (SEV_RANK[w] ?? 0) ? f.severity : w),
    "info"
  );
}
function sevDot(s: string): string {
  return s === "critical" ? "bg-red-400" : s === "warning" ? "bg-amber-400" : "bg-sky-400";
}
function sevBorder(s: string): string {
  return s === "critical" ? "border-red-600" : s === "warning" ? "border-amber-600" : "border-sky-700";
}
function sevPill(s: string): string {
  return s === "critical"
    ? "bg-red-500/20 text-red-300"
    : s === "warning"
    ? "bg-amber-500/20 text-amber-300"
    : "bg-sky-500/20 text-sky-300";
}

const SEVS = ["critical", "warning", "info"] as const;
/** Normalise an arbitrary model severity into one of the three known buckets. */
function normSev(s: string): string {
  return s === "critical" || s === "warning" ? s : "info";
}
function sevLabel(s: string): string {
  return s === "critical" ? "Critique" : s === "warning" ? "Avert." : "Info";
}

function AnnotationCard({ f }: { f: DiffFinding }) {
  return (
    <div
      className={`my-1 ml-6 mr-2 rounded-md border-l-2 ${sevBorder(
        f.severity
      )} bg-neutral-900/70 px-3 py-2 font-sans`}
    >
      <div className="flex items-center gap-2">
        <span className={`h-2 w-2 shrink-0 rounded-full ${sevDot(f.severity)}`} />
        <span className="text-[10px] font-semibold uppercase tracking-wide text-neutral-400">
          {f.severity}
        </span>
        <span className="text-xs font-medium text-neutral-100">{f.title}</span>
      </div>
      {f.detail.trim() && (
        <div className="mt-1 whitespace-pre-wrap text-xs leading-relaxed text-neutral-300">
          {f.detail}
        </div>
      )}
    </div>
  );
}

// ---- File tree (arborescence) ----

type TreeNode = {
  name: string;
  path: string;
  kind: "dir" | "file";
  status?: string;
  children: TreeNode[];
};

function buildTree(files: ChangedFile[]): TreeNode[] {
  const root: TreeNode = { name: "", path: "", kind: "dir", children: [] };
  for (const f of files) {
    const parts = f.path.split("/");
    let cur = root;
    parts.forEach((part, i) => {
      const isLeaf = i === parts.length - 1;
      const path = parts.slice(0, i + 1).join("/");
      let child = cur.children.find(
        (c) => c.name === part && (isLeaf ? c.kind === "file" : c.kind === "dir")
      );
      if (!child) {
        child = {
          name: part,
          path,
          kind: isLeaf ? "file" : "dir",
          status: isLeaf ? f.status : undefined,
          children: [],
        };
        cur.children.push(child);
      }
      cur = child;
    });
  }
  const sort = (n: TreeNode) => {
    n.children.sort((a, b) =>
      a.kind !== b.kind ? (a.kind === "dir" ? -1 : 1) : a.name.localeCompare(b.name)
    );
    n.children.forEach(sort);
  };
  sort(root);
  return root.children;
}

function TreeView({
  nodes,
  depth,
  selected,
  collapsed,
  counts,
  onToggle,
  onSelect,
}: {
  nodes: TreeNode[];
  depth: number;
  selected: string | null;
  collapsed: Set<string>;
  /** path → finding count + worst severity, for the badge. */
  counts: Record<string, { n: number; sev: string }>;
  onToggle: (path: string) => void;
  onSelect: (path: string) => void;
}) {
  return (
    <>
      {nodes.map((n) =>
        n.kind === "dir" ? (
          <div key={n.path}>
            <button
              onClick={() => onToggle(n.path)}
              style={{ paddingLeft: depth * 12 + 6 }}
              className="flex w-full items-center gap-1 py-0.5 pr-2 text-left text-xs text-neutral-400 hover:bg-neutral-800/60"
            >
              {collapsed.has(n.path) ? (
                <ChevronRight className="h-3 w-3 shrink-0" />
              ) : (
                <ChevronDown className="h-3 w-3 shrink-0" />
              )}
              <span className="truncate">{n.name}</span>
            </button>
            {!collapsed.has(n.path) && (
              <TreeView
                nodes={n.children}
                depth={depth + 1}
                selected={selected}
                collapsed={collapsed}
                counts={counts}
                onToggle={onToggle}
                onSelect={onSelect}
              />
            )}
          </div>
        ) : (
          <button
            key={n.path}
            onClick={() => onSelect(n.path)}
            style={{ paddingLeft: depth * 12 + 6 + 16 }}
            title={n.path}
            className={`flex w-full items-center gap-1.5 py-0.5 pr-2 text-left text-xs ${
              selected === n.path
                ? "bg-indigo-500/15 text-neutral-100"
                : "text-neutral-300 hover:bg-neutral-800/60"
            }`}
          >
            <span className={`w-3 shrink-0 text-center font-mono ${statusColor(n.status)}`}>
              {n.status ? n.status.slice(0, 1) : "•"}
            </span>
            <span className="min-w-0 flex-1 truncate">{n.name}</span>
            {counts[n.path] && (
              <span
                className={`shrink-0 rounded-full px-1.5 text-[9px] font-semibold ${sevPill(
                  counts[n.path].sev
                )}`}
                title={`${counts[n.path].n} remarque(s) IA`}
              >
                {counts[n.path].n}
              </span>
            )}
          </button>
        )
      )}
    </>
  );
}

// ---- Numbered unified diff ----

type DiffRowKind = "ctx" | "add" | "del" | "hunk" | "meta";
type DiffRow = { oldNo: number | null; newNo: number | null; kind: DiffRowKind; text: string };

/** Parse one file's unified-diff chunk into rows carrying old/new line numbers. */
function parseNumberedDiff(text: string): DiffRow[] {
  const rows: DiffRow[] = [];
  let oldNo = 0;
  let newNo = 0;
  for (const line of text.split("\n")) {
    if (line.startsWith("@@")) {
      const m = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (m) {
        oldNo = parseInt(m[1], 10);
        newNo = parseInt(m[2], 10);
      }
      rows.push({ oldNo: null, newNo: null, kind: "hunk", text: line });
    } else if (
      line.startsWith("diff ") ||
      line.startsWith("index ") ||
      line.startsWith("--- ") ||
      line.startsWith("+++ ") ||
      line.startsWith("new file") ||
      line.startsWith("deleted file") ||
      line.startsWith("old mode") ||
      line.startsWith("new mode") ||
      line.startsWith("similarity") ||
      line.startsWith("rename ") ||
      line.startsWith("copy ")
    ) {
      // diff noise — the filename + counts are shown in the file header instead.
    } else if (line.startsWith("Binary ")) {
      rows.push({ oldNo: null, newNo: null, kind: "meta", text: line });
    } else if (line.startsWith("+")) {
      rows.push({ oldNo: null, newNo, kind: "add", text: line.slice(1) });
      newNo++;
    } else if (line.startsWith("-")) {
      rows.push({ oldNo, newNo: null, kind: "del", text: line.slice(1) });
      oldNo++;
    } else if (line.startsWith("\\")) {
      rows.push({ oldNo: null, newNo: null, kind: "meta", text: line });
    } else {
      const t = line.startsWith(" ") ? line.slice(1) : line;
      rows.push({ oldNo, newNo, kind: "ctx", text: t });
      oldNo++;
      newNo++;
    }
  }
  if (rows.length && rows[rows.length - 1].kind === "ctx" && rows[rows.length - 1].text === "") {
    rows.pop();
  }
  return rows;
}

// ---- Lightweight, language-agnostic syntax highlighting ----
// No dependency / no per-language grammar: a single scanner that recognises the tokens
// common to most languages (comments, strings, numbers, keywords, capitalised types).
// Good enough to read a diff; never throws on unknown syntax.

const KEYWORDS = new Set([
  "if", "else", "elif", "for", "while", "do", "switch", "case", "default", "break",
  "continue", "return", "goto", "yield", "await", "async", "function", "fn", "def",
  "func", "lambda", "class", "struct", "enum", "interface", "trait", "impl", "type",
  "typedef", "namespace", "module", "mod", "package", "use", "using", "import", "export",
  "from", "as", "require", "const", "let", "var", "val", "static", "final", "public",
  "private", "protected", "internal", "abstract", "virtual", "override", "new", "delete",
  "extends", "implements", "throws", "throw", "try", "catch", "finally", "except", "raise",
  "with", "match", "when", "where", "in", "of", "is", "not", "and", "or", "instanceof",
  "typeof", "void", "sizeof", "defer", "go", "select", "chan", "make", "pub", "unsafe",
  "ref", "mut", "dyn", "macro", "begin", "end", "then", "echo", "local",
]);
const CONSTANTS = new Set([
  "true", "false", "null", "nil", "none", "None", "True", "False", "undefined", "NaN",
  "Infinity", "this", "self", "super",
]);
const TOKEN_CLS: Record<string, string> = {
  cmt: "text-neutral-500 italic",
  str: "text-amber-300",
  num: "text-sky-300",
  kw: "text-violet-300",
  cst: "text-orange-300",
  typ: "text-cyan-300",
  punct: "text-neutral-400",
};

const WS = (c: string) => c === " " || c === "\t";
const IDENT_START = (c: string) => /[A-Za-z_$]/.test(c);
const IDENT_PART = (c: string) => /[A-Za-z0-9_$]/.test(c);
const NUM_PART = (c: string) => /[0-9a-fA-FxXob._]/.test(c);

/** Split a single code line into classified tokens (see TOKEN_CLS keys; "" = plain). */
function tokenize(src: string): { text: string; cls: string }[] {
  const out: { text: string; cls: string }[] = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (WS(c)) {
      let j = i + 1;
      while (j < n && WS(src[j])) j++;
      out.push({ text: src.slice(i, j), cls: "" });
      i = j;
    } else if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      const j = end === -1 ? n : end + 2;
      out.push({ text: src.slice(i, j), cls: "cmt" });
      i = j;
    } else if (c === "/" && src[i + 1] === "/") {
      out.push({ text: src.slice(i), cls: "cmt" });
      i = n;
    } else if (c === "#" && src.slice(0, i).trim() === "") {
      // `#` at the start of the line → comment (Python/shell/yaml/`#!`); mid-line `#`
      // (private fields, hex colours) falls through to punctuation.
      out.push({ text: src.slice(i), cls: "cmt" });
      i = n;
    } else if (c === '"' || c === "'" || c === "`") {
      let j = i + 1;
      while (j < n) {
        if (src[j] === "\\") {
          j += 2;
          continue;
        }
        if (src[j] === c) {
          j++;
          break;
        }
        j++;
      }
      out.push({ text: src.slice(i, j), cls: "str" });
      i = j;
    } else if (/[0-9]/.test(c) || (c === "." && /[0-9]/.test(src[i + 1] ?? ""))) {
      let j = i + 1;
      while (j < n && NUM_PART(src[j])) j++;
      out.push({ text: src.slice(i, j), cls: "num" });
      i = j;
    } else if (IDENT_START(c)) {
      let j = i + 1;
      while (j < n && IDENT_PART(src[j])) j++;
      const w = src.slice(i, j);
      const cls = KEYWORDS.has(w)
        ? "kw"
        : CONSTANTS.has(w)
        ? "cst"
        : /^[A-Z]/.test(w)
        ? "typ"
        : "";
      out.push({ text: w, cls });
      i = j;
    } else {
      out.push({ text: c, cls: "punct" });
      i += 1;
    }
  }
  return out;
}

/** Render a code line with lightweight syntax highlighting (inside a `whitespace-pre*`). */
function Code({ text }: { text: string }) {
  if (!text) return <>{" "}</>;
  return (
    <>
      {tokenize(text).map((t, i) => (
        <span key={i} className={TOKEN_CLS[t.cls] || undefined}>
          {t.text}
        </span>
      ))}
    </>
  );
}

function NumberedDiff({
  rows,
  byLine,
}: {
  rows: DiffRow[];
  byLine: Map<number, DiffFinding[]>;
}) {
  return (
    <div className="overflow-x-auto rounded-md border border-neutral-800 bg-neutral-950 font-mono text-[11px] leading-relaxed">
      {rows.map((r, i) => {
        let rowEl;
        if (r.kind === "hunk") {
          rowEl = (
            <div className="whitespace-pre bg-neutral-900/60 px-2 py-0.5 text-cyan-300/80">
              {r.text}
            </div>
          );
        } else if (r.kind === "meta") {
          rowEl = <div className="whitespace-pre px-2 text-neutral-600">{r.text}</div>;
        } else {
          const bg =
            r.kind === "add" ? "bg-emerald-950/30" : r.kind === "del" ? "bg-red-950/30" : "";
          const signCls =
            r.kind === "add"
              ? "text-emerald-400"
              : r.kind === "del"
              ? "text-red-400"
              : "text-neutral-600";
          const sign = r.kind === "add" ? "+" : r.kind === "del" ? "-" : " ";
          rowEl = (
            <div className={`flex ${bg}`}>
              <span className="w-10 shrink-0 select-none border-r border-neutral-800/60 px-1 text-right text-neutral-600">
                {r.oldNo ?? ""}
              </span>
              <span className="w-10 shrink-0 select-none border-r border-neutral-800/60 px-1 text-right text-neutral-600">
                {r.newNo ?? ""}
              </span>
              <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
              <span className="whitespace-pre text-neutral-200">
                <Code text={r.text} />
              </span>
            </div>
          );
        }
        const anns = r.newNo != null ? byLine.get(r.newNo) : undefined;
        return (
          <Fragment key={i}>
            {rowEl}
            {anns?.map((f, k) => <AnnotationCard key={k} f={f} />)}
          </Fragment>
        );
      })}
    </div>
  );
}

// ---- Side-by-side (split) diff ----

type Cell = { no: number | null; text: string; kind: "ctx" | "add" | "del" };
type SplitRow = { full?: string; fullKind?: "hunk" | "meta"; l?: Cell; r?: Cell };

/** Pair unified rows into left (old) / right (new) columns for a split view. */
function toSplitRows(rows: DiffRow[]): SplitRow[] {
  const out: SplitRow[] = [];
  let i = 0;
  while (i < rows.length) {
    const r = rows[i];
    if (r.kind === "hunk" || r.kind === "meta") {
      out.push({ full: r.text, fullKind: r.kind });
      i++;
    } else if (r.kind === "ctx") {
      out.push({
        l: { no: r.oldNo, text: r.text, kind: "ctx" },
        r: { no: r.newNo, text: r.text, kind: "ctx" },
      });
      i++;
    } else {
      const dels: DiffRow[] = [];
      const adds: DiffRow[] = [];
      while (i < rows.length && rows[i].kind === "del") dels.push(rows[i++]);
      while (i < rows.length && rows[i].kind === "add") adds.push(rows[i++]);
      const n = Math.max(dels.length, adds.length);
      for (let k = 0; k < n; k++) {
        const d = dels[k];
        const a = adds[k];
        out.push({
          l: d ? { no: d.oldNo, text: d.text, kind: "del" } : undefined,
          r: a ? { no: a.newNo, text: a.text, kind: "add" } : undefined,
        });
      }
    }
  }
  return out;
}

function HalfCell({ cell }: { cell?: Cell }) {
  const bg = !cell
    ? "bg-neutral-900/20"
    : cell.kind === "del"
    ? "bg-red-950/30"
    : cell.kind === "add"
    ? "bg-emerald-950/30"
    : "";
  return (
    <div className={`flex w-1/2 min-w-0 ${bg}`}>
      <span className="w-9 shrink-0 select-none border-r border-neutral-800/60 px-1 text-right text-neutral-600">
        {cell?.no ?? ""}
      </span>
      <span className="min-w-0 flex-1 whitespace-pre-wrap break-all px-1 text-neutral-200">
        {cell ? <Code text={cell.text} /> : ""}
      </span>
    </div>
  );
}

function SplitDiff({
  rows,
  byLine,
}: {
  rows: DiffRow[];
  byLine: Map<number, DiffFinding[]>;
}) {
  const split = useMemo(() => toSplitRows(rows), [rows]);
  return (
    <div className="rounded-md border border-neutral-800 bg-neutral-950 font-mono text-[11px] leading-relaxed">
      {split.map((r, i) => {
        const rowEl =
          r.full != null ? (
            <div
              className={`whitespace-pre-wrap break-all px-2 py-0.5 ${
                r.fullKind === "hunk" ? "bg-neutral-900/60 text-cyan-300/80" : "text-neutral-600"
              }`}
            >
              {r.full}
            </div>
          ) : (
            <div className="flex">
              <HalfCell cell={r.l} />
              <div className="w-px shrink-0 bg-neutral-800" />
              <HalfCell cell={r.r} />
            </div>
          );
        const anns = r.r?.no != null ? byLine.get(r.r.no) : undefined;
        return (
          <Fragment key={i}>
            {rowEl}
            {anns?.map((f, k) => <AnnotationCard key={k} f={f} />)}
          </Fragment>
        );
      })}
    </div>
  );
}

/** Split a unified diff into per-file chunks, keyed by the (b/) path. */
export function splitDiffByFile(diff: string): Record<string, string> {
  const out: Record<string, string> = {};
  if (!diff) return out;
  let current: string | null = null;
  let buf: string[] = [];
  const flush = () => {
    if (current) out[current] = buf.join("\n");
  };
  for (const line of diff.split("\n")) {
    if (line.startsWith("diff --git ")) {
      flush();
      const m = line.match(/ b\/(.+)$/);
      current = m ? m[1] : line;
      buf = [];
    }
    buf.push(line);
  }
  flush();
  return out;
}

/** Count +/- lines in a file's unified-diff chunk (ignoring the +++/--- headers). */
function countChanges(chunk: string): { add: number; del: number } {
  let add = 0;
  let del = 0;
  for (const l of chunk.split("\n")) {
    if (l.startsWith("+") && !l.startsWith("+++")) add++;
    else if (l.startsWith("-") && !l.startsWith("---")) del++;
  }
  return { add, del };
}

// ---- The explorer: file tree (left) + selected file's diff (right) ----

export function DiffExplorer({
  files,
  diffByFile,
  selected,
  onSelect,
  view,
  findings = [],
}: {
  files: ChangedFile[];
  /** path → that file's unified-diff chunk. */
  diffByFile: Record<string, string>;
  selected: string | null;
  onSelect: (path: string) => void;
  view: DiffViewMode;
  /** AI-review findings to annotate inline + badge in the tree. */
  findings?: DiffFinding[];
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const tree = useMemo(() => buildTree(files), [files]);
  const chunk = selected ? diffByFile[selected] ?? "" : "";
  const counts = useMemo(() => countChanges(chunk), [chunk]);
  const rows = useMemo(() => parseNumberedDiff(chunk), [chunk]);

  // Severity filter: chips toggle which findings are shown (inline + tree badges).
  const [activeSev, setActiveSev] = useState<Set<string>>(() => new Set(SEVS));
  const sevCounts = useMemo(() => {
    const c: Record<string, number> = { critical: 0, warning: 0, info: 0 };
    for (const f of findings) c[normSev(f.severity)]++;
    return c;
  }, [findings]);
  const shown = useMemo(
    () => findings.filter((f) => activeSev.has(normSev(f.severity))),
    [findings, activeSev]
  );
  const toggleSev = (s: string) =>
    setActiveSev((prev) => {
      const next = new Set(prev);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });

  // Shown findings grouped by file (for tree badges) and, for the selected file, by line.
  const byFile = useMemo(() => {
    const m: Record<string, DiffFinding[]> = {};
    for (const f of shown) {
      if (!f.file) continue;
      (m[f.file] ??= []).push(f);
    }
    return m;
  }, [shown]);
  const badgeCounts = useMemo(() => {
    const c: Record<string, { n: number; sev: string }> = {};
    for (const [p, fs] of Object.entries(byFile)) c[p] = { n: fs.length, sev: worstSeverity(fs) };
    return c;
  }, [byFile]);

  const fileFindings = selected ? byFile[selected] ?? [] : [];
  const presentLines = useMemo(
    () => new Set(rows.filter((r) => r.newNo != null).map((r) => r.newNo as number)),
    [rows]
  );
  const byLine = useMemo(() => {
    const m = new Map<number, DiffFinding[]>();
    for (const f of fileFindings) {
      if (f.line != null && presentLines.has(f.line)) {
        const a = m.get(f.line) ?? [];
        a.push(f);
        m.set(f.line, a);
      }
    }
    return m;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected, shown, presentLines]);
  // Findings with no line (or a line not in the diff) shown as cards above the diff.
  const loose = fileFindings.filter((f) => f.line == null || !presentLines.has(f.line));

  function toggleDir(path: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {findings.length > 0 && (
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-neutral-800 bg-neutral-900/30 px-3 py-1.5 text-[11px]">
          <span className="text-neutral-500">Remarques IA :</span>
          {SEVS.map((s) =>
            sevCounts[s] > 0 ? (
              <button
                key={s}
                onClick={() => toggleSev(s)}
                title={activeSev.has(s) ? `Masquer « ${sevLabel(s)} »` : `Afficher « ${sevLabel(s)} »`}
                className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 ${
                  activeSev.has(s)
                    ? `border-transparent ${sevPill(s)}`
                    : "border-neutral-700 text-neutral-500 line-through"
                }`}
              >
                <span className={`h-1.5 w-1.5 rounded-full ${sevDot(s)}`} />
                {sevLabel(s)} {sevCounts[s]}
              </button>
            ) : null
          )}
          <span className="ml-auto text-neutral-600">
            {shown.length}/{findings.length} affichée(s)
          </span>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <aside className="w-64 shrink-0 overflow-auto border-r border-neutral-800 py-2">
        <div className="px-3 pb-2 text-[10px] uppercase tracking-wider text-neutral-500">
          {files.length} fichier(s)
        </div>
        {files.length === 0 && <p className="px-3 text-xs text-neutral-600">Aucun changement.</p>}
        <TreeView
          nodes={tree}
          depth={0}
          selected={selected}
          collapsed={collapsed}
          counts={badgeCounts}
          onToggle={toggleDir}
          onSelect={onSelect}
        />
      </aside>

      <div className="min-w-0 flex-1 overflow-auto p-4">
        {selected ? (
          <>
            <div className="mb-2 flex items-center gap-2 text-xs">
              <span className="truncate font-mono text-neutral-200">{selected}</span>
              <span className="ml-auto shrink-0 font-mono text-[11px]">
                <span className="text-emerald-400">+{counts.add}</span>{" "}
                <span className="text-red-400">−{counts.del}</span>
              </span>
            </div>
            {loose.length > 0 && (
              <div className="mb-2">
                {loose.map((f, i) => (
                  <AnnotationCard key={i} f={f} />
                ))}
              </div>
            )}
            {chunk.trim() ? (
              view === "split" ? (
                <SplitDiff rows={rows} byLine={byLine} />
              ) : (
                <NumberedDiff rows={rows} byLine={byLine} />
              )
            ) : (
              loose.length === 0 && (
                <p className="text-xs text-neutral-600">
                  Pas de diff texte pour ce fichier (binaire ou renommage).
                </p>
              )
            )}
          </>
        ) : (
          <p className="text-xs text-neutral-600">Sélectionne un fichier à gauche.</p>
        )}
        </div>
      </div>
    </div>
  );
}
