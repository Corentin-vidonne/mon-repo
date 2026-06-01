import { useState } from "react";
import { FolderOpen, DownloadCloud } from "lucide-react";
import { Modal } from "./Modal";
import { api, errorText, pickRepoFolder } from "../lib/api";
import type { RepoView } from "../lib/types";

const inputClass =
  "w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-indigo-600";

/** Folder name git clone would create from a URL (preview only). */
function deriveName(url: string): string {
  const u = url.trim().replace(/[/\\]+$/, "");
  const seg = u.split(/[/\\:]/).pop() ?? "";
  return seg.replace(/\.git$/, "");
}

export function AddRepoDialog({
  onDone,
  onClose,
}: {
  onDone: (view: RepoView) => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"open" | "clone">("open");
  const [url, setUrl] = useState("");
  const [dest, setDest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function openExisting() {
    const dir = await pickRepoFolder("Select an existing git repository");
    if (!dir) return;
    setBusy(true);
    setError(null);
    try {
      onDone(await api.getRepoView(dir));
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  }

  async function chooseDest() {
    const dir = await pickRepoFolder("Choose where to clone the repository");
    if (dir) setDest(dir);
  }

  async function doClone() {
    if (!url.trim() || !dest) return;
    setBusy(true);
    setError(null);
    try {
      onDone(await api.cloneRepo(url.trim(), dest));
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  }

  const name = deriveName(url);

  const tabBtn = (id: "open" | "clone", label: string, icon: React.ReactNode) => (
    <button
      onClick={() => setTab(id)}
      className={`flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium ${
        tab === id
          ? "bg-neutral-800 text-neutral-100"
          : "text-neutral-400 hover:text-neutral-200"
      }`}
    >
      {icon}
      {label}
    </button>
  );

  return (
    <Modal title="Add repository" onClose={onClose}>
      <div className="mb-3 flex gap-1 rounded-md border border-neutral-800 p-0.5">
        {tabBtn("open", "Open existing", <FolderOpen className="h-4 w-4" />)}
        {tabBtn("clone", "Clone from URL", <DownloadCloud className="h-4 w-4" />)}
      </div>

      {error && (
        <div className="mb-3 rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
          {error}
        </div>
      )}

      {tab === "open" ? (
        <div className="space-y-3">
          <p className="text-xs text-neutral-400">
            Pick a repository already cloned on your machine.
          </p>
          <button
            onClick={openExisting}
            disabled={busy}
            className="flex w-full items-center justify-center gap-2 rounded-md bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            <FolderOpen className="h-4 w-4" />
            {busy ? "Opening…" : "Choose folder…"}
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-xs text-neutral-400">Repository URL</label>
            <input
              autoFocus
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://github.com/owner/repo.git"
              className={inputClass}
            />
          </div>
          <div>
            <label className="mb-1 block text-xs text-neutral-400">Clone into</label>
            <button
              onClick={chooseDest}
              className="flex w-full items-center gap-2 rounded-md border border-neutral-700 px-3 py-1.5 text-left text-sm text-neutral-200 hover:bg-neutral-800"
            >
              <FolderOpen className="h-4 w-4 shrink-0 text-neutral-400" />
              <span className="truncate">{dest ?? "Choose destination folder…"}</span>
            </button>
            {dest && name && (
              <p className="mt-1 truncate text-xs text-neutral-500">
                → {dest}\{name}
              </p>
            )}
          </div>
          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md px-3 py-1.5 text-sm text-neutral-400 hover:bg-neutral-800"
            >
              Cancel
            </button>
            <button
              onClick={doClone}
              disabled={busy || !url.trim() || !dest}
              className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              <DownloadCloud className="h-4 w-4" />
              {busy ? "Cloning…" : "Clone"}
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
