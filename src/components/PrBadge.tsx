import { openUrl } from "@tauri-apps/plugin-opener";
import type { PrInfo } from "../lib/types";

const STATE_STYLES: Record<string, string> = {
  OPEN: "border-emerald-700 bg-emerald-950/50 text-emerald-300",
  MERGED: "border-purple-700 bg-purple-950/50 text-purple-300",
  CLOSED: "border-red-800 bg-red-950/50 text-red-300",
};

function ciColor(c: string): string {
  if (c === "SUCCESS") return "text-emerald-400";
  if (c === "FAILURE") return "text-red-400";
  return "text-amber-400";
}

export function PrBadge({ pr }: { pr: PrInfo }) {
  const style =
    STATE_STYLES[pr.state] ?? "border-neutral-700 bg-neutral-900 text-neutral-300";
  return (
    <button
      onClick={() => openUrl(pr.url)}
      title={`${pr.state}${pr.reviewDecision ? " · " + pr.reviewDecision : ""}${
        pr.checks ? " · CI " + pr.checks : ""
      } — open on GitHub`}
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium hover:brightness-125 ${style}`}
    >
      {pr.checks && <span className={ciColor(pr.checks)}>●</span>}#{pr.number}
    </button>
  );
}
