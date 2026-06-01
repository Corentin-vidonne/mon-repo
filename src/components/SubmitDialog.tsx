import { useEffect, useState } from "react";
import { Modal } from "./Modal";
import { api, errorText } from "../lib/api";
import type { RepoView, SubmitStepInfo } from "../lib/types";

export function SubmitDialog({
  repoPath,
  onDone,
  onClose,
}: {
  repoPath: string;
  onDone: (view: RepoView, summary: string) => void;
  onClose: () => void;
}) {
  const [steps, setSteps] = useState<SubmitStepInfo[] | null>(null);
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .submitPlan(repoPath, null)
      .then((s) => {
        if (!alive) return;
        setSteps(s);
        const t: Record<string, string> = {};
        s.forEach((st) => {
          if (st.action === "create") t[st.branch] = st.defaultTitle;
        });
        setTitles(t);
      })
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [repoPath]);

  const creates = steps?.filter((s) => s.action === "create").length ?? 0;
  const updates = steps?.filter((s) => s.action === "update").length ?? 0;

  async function confirm() {
    setSubmitting(true);
    setError(null);
    try {
      const view = await api.submit(repoPath, null, draft, titles);
      const parts = [];
      if (creates) parts.push(`${creates} PR created`);
      if (updates) parts.push(`${updates} updated`);
      onDone(view, parts.join(", ") || "submitted");
    } catch (e) {
      setError(errorText(e));
      setSubmitting(false);
    }
  }

  const badge = (action: string) =>
    action === "create"
      ? "bg-emerald-950/50 text-emerald-300"
      : action === "update"
      ? "bg-amber-950/50 text-amber-300"
      : "bg-neutral-800 text-neutral-400";

  return (
    <Modal title="Submit stack" onClose={onClose}>
      {steps === null && !error && (
        <p className="text-sm text-neutral-500">Loading plan…</p>
      )}
      {error && (
        <div className="mb-3 rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
          {error}
        </div>
      )}
      {steps && steps.length === 0 && (
        <p className="text-sm text-neutral-500">Nothing to submit — no tracked branches.</p>
      )}
      {steps && steps.length > 0 && (
        <div className="space-y-3">
          <div className="max-h-72 space-y-2 overflow-auto">
            {steps.map((s) => (
              <div key={s.branch} className="rounded-md border border-neutral-800 p-2">
                <div className="flex items-center gap-2 text-xs">
                  <span className="font-mono text-neutral-200">{s.branch}</span>
                  <span className="text-neutral-500">→ {s.base}</span>
                  <span className={`ml-auto rounded px-1.5 py-0.5 text-[10px] ${badge(s.action)}`}>
                    {s.action === "create"
                      ? "new PR"
                      : s.action === "update"
                      ? "update base"
                      : "up to date"}
                  </span>
                </div>
                {s.action === "create" && (
                  <input
                    value={titles[s.branch] ?? ""}
                    onChange={(e) =>
                      setTitles((t) => ({ ...t, [s.branch]: e.target.value }))
                    }
                    placeholder="PR title"
                    className="mt-1.5 w-full rounded border border-neutral-700 bg-neutral-950 px-2 py-1 text-xs text-neutral-100 outline-none focus:border-indigo-600"
                  />
                )}
              </div>
            ))}
          </div>

          <label className="flex items-center gap-2 text-xs text-neutral-300">
            <input
              type="checkbox"
              checked={draft}
              onChange={(e) => setDraft(e.target.checked)}
            />
            Create new PRs as draft
          </label>

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md px-3 py-1.5 text-sm text-neutral-400 hover:bg-neutral-800"
            >
              Cancel
            </button>
            <button
              onClick={confirm}
              disabled={submitting || (creates === 0 && updates === 0)}
              className="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
            >
              {submitting ? "Submitting…" : "Submit"}
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
