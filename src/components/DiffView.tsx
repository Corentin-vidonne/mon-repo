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

export function DiffView({ text }: { text: string }) {
  const lines = text.split("\n");
  return (
    <pre className="overflow-x-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 font-mono text-[11px] leading-relaxed">
      {lines.map((l, i) => {
        let cls = "text-neutral-400";
        if (l.startsWith("@@")) cls = "text-cyan-300";
        else if (l.startsWith("+++") || l.startsWith("---")) cls = "text-neutral-500";
        else if (l.startsWith("diff ") || l.startsWith("index ")) cls = "text-neutral-600";
        else if (l.startsWith("+")) cls = "bg-emerald-950/30 text-emerald-300";
        else if (l.startsWith("-")) cls = "bg-red-950/30 text-red-300";
        return (
          <div key={i} className={`whitespace-pre ${cls}`}>
            {l || " "}
          </div>
        );
      })}
    </pre>
  );
}
