import { safeOpen } from "../lib/safeOpen";
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

/** A compact mark for a PR's review decision (approved ✓ / changes requested ✗). */
export function reviewMark(
  decision: string | null
): { ch: string; cls: string } | null {
  if (decision === "APPROVED") return { ch: "✓", cls: "text-emerald-400" };
  if (decision === "CHANGES_REQUESTED") return { ch: "✗", cls: "text-red-400" };
  return null;
}

export function PrBadge({ pr }: { pr: PrInfo }) {
  const style =
    STATE_STYLES[pr.state] ?? "border-neutral-700 bg-neutral-900 text-neutral-300";
  const rev = reviewMark(pr.reviewDecision);
  return (
    <button
      onClick={() => safeOpen(pr.url)}
      title={`${pr.state}${pr.reviewDecision ? " · " + pr.reviewDecision : ""}${
        pr.checks ? " · CI " + pr.checks : ""
      } — open on GitHub`}
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium hover:brightness-125 ${style}`}
    >
      {pr.checks && <span className={ciColor(pr.checks)}>●</span>}#{pr.number}
      {rev && <span className={`ml-0.5 font-semibold ${rev.cls}`}>{rev.ch}</span>}
    </button>
  );
}
