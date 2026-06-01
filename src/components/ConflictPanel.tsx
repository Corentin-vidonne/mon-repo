import { AlertTriangle, Check, X } from "lucide-react";
import type { ConflictState } from "../lib/types";

export function ConflictPanel({
  conflict,
  busy,
  onContinue,
  onAbort,
}: {
  conflict: ConflictState;
  busy: boolean;
  onContinue: () => void;
  onAbort: () => void;
}) {
  return (
    <div className="mb-4 rounded-lg border border-amber-800 bg-amber-950/30 p-4">
      <div className="flex items-center gap-2 text-amber-300">
        <AlertTriangle className="h-5 w-5 shrink-0" />
        <h3 className="text-sm font-semibold">
          Restack paused — resolve conflicts
          {conflict.branch ? (
            <>
              {" "}
              on <span className="font-mono">{conflict.branch}</span>
            </>
          ) : null}
        </h3>
      </div>
      <p className="mt-2 text-xs text-neutral-400">
        Fix the conflicts in your editor, stage them (
        <code className="rounded bg-neutral-800 px-1">git add</code>), then continue.
      </p>
      {conflict.files.length > 0 && (
        <ul className="mt-2 space-y-0.5">
          {conflict.files.map((f) => (
            <li key={f} className="font-mono text-xs text-amber-200">
              {f}
            </li>
          ))}
        </ul>
      )}
      <div className="mt-3 flex gap-2">
        <button
          onClick={onContinue}
          disabled={busy}
          className="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
        >
          <Check className="h-3.5 w-3.5" /> Continue restack
        </button>
        <button
          onClick={onAbort}
          disabled={busy}
          className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-xs font-medium text-neutral-300 hover:bg-neutral-800 disabled:opacity-50"
        >
          <X className="h-3.5 w-3.5" /> Abort
        </button>
      </div>
    </div>
  );
}
