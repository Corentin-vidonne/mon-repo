import { useCallback, useEffect, useState } from "react";
import Markdown from "react-markdown";
import { FileText, Plus, Eye, Code } from "lucide-react";
import type { RepoView } from "../lib/types";
import { api, errorText } from "../lib/api";

type Mode = "view" | "new";
type Render = "rendered" | "raw";

export function DocsView({
  repoPath,
  branches,
  defaultBranch,
  onCreated,
}: {
  repoPath: string;
  branches: string[];
  defaultBranch: string;
  onCreated: (view: RepoView) => void;
}) {
  const [files, setFiles] = useState<string[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState<string>("");
  const [render, setRender] = useState<Render>("rendered");
  const [mode, setMode] = useState<Mode>("view");
  const [error, setError] = useState<string | null>(null);

  // New-file form state.
  const [newPath, setNewPath] = useState("");
  const [newBranch, setNewBranch] = useState(defaultBranch);
  const [newContent, setNewContent] = useState("");
  const [saving, setSaving] = useState(false);

  const loadList = useCallback(() => {
    api
      .listMarkdown(repoPath)
      .then(setFiles)
      .catch((e) => setError(errorText(e)));
  }, [repoPath]);

  useEffect(() => {
    setFiles(null);
    setSelected(null);
    setContent("");
    setMode("view");
    loadList();
  }, [loadList]);

  function openFile(rel: string) {
    setMode("view");
    setSelected(rel);
    setContent("");
    setError(null);
    api
      .readMarkdown(repoPath, rel)
      .then(setContent)
      .catch((e) => setError(errorText(e)));
  }

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const view = await api.createMarkdown(repoPath, newBranch, newPath, newContent);
      onCreated(view);
      loadList();
      setMode("view");
      setSelected(newPath.trim());
      setContent(newContent);
      setNewPath("");
      setNewContent("");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full">
      {/* File list */}
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-800">
        <div className="flex items-center justify-between px-3 py-2">
          <span className="text-xs uppercase tracking-wider text-neutral-500">
            Markdown
          </span>
          <button
            onClick={() => {
              setMode("new");
              setNewBranch(defaultBranch);
            }}
            title="New .md file"
            className="inline-flex items-center gap-1 rounded bg-indigo-600 px-2 py-1 text-xs font-medium text-white hover:bg-indigo-500"
          >
            <Plus className="h-3 w-3" /> New
          </button>
        </div>
        <div className="flex-1 overflow-auto px-2 pb-2">
          {!files && <p className="px-2 text-sm text-neutral-500">Loading…</p>}
          {files && files.length === 0 && (
            <p className="px-2 text-sm text-neutral-600">No .md files.</p>
          )}
          {files?.map((f) => (
            <button
              key={f}
              onClick={() => openFile(f)}
              title={f}
              className={`flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-xs ${
                selected === f && mode === "view"
                  ? "bg-neutral-800 text-neutral-100"
                  : "text-neutral-400 hover:bg-neutral-900"
              }`}
            >
              <FileText className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
              <span className="truncate">{f}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Right pane */}
      <div className="flex min-w-0 flex-1 flex-col">
        {error && (
          <div className="m-3 rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
            {error}
          </div>
        )}

        {mode === "new" ? (
          <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={newPath}
                onChange={(e) => setNewPath(e.target.value)}
                placeholder="path/to/file.md"
                className="flex-1 rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-indigo-600"
              />
              <select
                value={newBranch}
                onChange={(e) => setNewBranch(e.target.value)}
                title="Commit on branch"
                className="rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1.5 text-sm text-neutral-100 outline-none focus:border-indigo-600"
              >
                {branches.map((b) => (
                  <option key={b} value={b}>
                    {b}
                  </option>
                ))}
              </select>
              <button
                onClick={save}
                disabled={saving || !newPath.trim()}
                className="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
              >
                {saving ? "Committing…" : "Create & commit"}
              </button>
              <button
                onClick={() => setMode("view")}
                className="rounded-md px-3 py-1.5 text-sm text-neutral-400 hover:bg-neutral-800"
              >
                Cancel
              </button>
            </div>
            <div className="grid min-h-0 flex-1 grid-cols-2 gap-3">
              <textarea
                value={newContent}
                onChange={(e) => setNewContent(e.target.value)}
                placeholder="# Title&#10;&#10;Write Markdown here…"
                className="min-h-0 resize-none rounded-md border border-neutral-800 bg-neutral-950 p-3 font-mono text-sm text-neutral-100 outline-none focus:border-indigo-600"
              />
              <div className="min-h-0 overflow-auto rounded-md border border-neutral-800 bg-neutral-950/40 p-3">
                <div className="markdown-body">
                  <Markdown>{newContent || "*Preview*"}</Markdown>
                </div>
              </div>
            </div>
          </div>
        ) : selected ? (
          <>
            <div className="flex items-center gap-2 border-b border-neutral-800 px-4 py-2">
              <FileText className="h-4 w-4 text-neutral-500" />
              <span className="truncate font-mono text-sm text-neutral-200">{selected}</span>
              <div className="ml-auto flex rounded-md border border-neutral-700 p-0.5">
                <button
                  onClick={() => setRender("rendered")}
                  title="Rendered"
                  className={`rounded p-1 ${
                    render === "rendered"
                      ? "bg-neutral-700 text-neutral-100"
                      : "text-neutral-400 hover:text-neutral-200"
                  }`}
                >
                  <Eye className="h-4 w-4" />
                </button>
                <button
                  onClick={() => setRender("raw")}
                  title="Raw"
                  className={`rounded p-1 ${
                    render === "raw"
                      ? "bg-neutral-700 text-neutral-100"
                      : "text-neutral-400 hover:text-neutral-200"
                  }`}
                >
                  <Code className="h-4 w-4" />
                </button>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-auto p-5">
              {render === "rendered" ? (
                <div className="markdown-body mx-auto max-w-3xl">
                  <Markdown>{content}</Markdown>
                </div>
              ) : (
                <pre className="mx-auto max-w-3xl whitespace-pre-wrap break-words font-mono text-xs text-neutral-200">
                  {content}
                </pre>
              )}
            </div>
          </>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-neutral-600">
            Select a .md file, or create a new one.
          </div>
        )}
      </div>
    </div>
  );
}
