import { useEffect, useRef, useState } from "react";
import { ListFilter, Check } from "lucide-react";

/**
 * Branch filter for the commit graph. `value === null` means "all branches".
 * Otherwise it's the explicit list of branch names to show.
 */
export function CommitFilter({
  branches,
  value,
  onChange,
}: {
  branches: string[];
  value: string[] | null;
  onChange: (next: string[] | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  const isAll = value === null;
  const selected = new Set(isAll ? branches : value);

  function toggle(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    // If everything ends up selected, collapse back to "all" (null).
    if (next.size === branches.length) onChange(null);
    else onChange([...next]);
  }

  const label = isAll ? "All branches" : `${selected.size}/${branches.length} branches`;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={`inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs font-medium shadow-sm ${
          isAll
            ? "border-neutral-700 bg-neutral-900 text-neutral-200"
            : "border-indigo-600 bg-indigo-950/50 text-indigo-200"
        } hover:bg-neutral-800`}
      >
        <ListFilter className="h-3.5 w-3.5" />
        {label}
      </button>

      {open && (
        <div className="absolute left-0 z-20 mt-1 w-56 rounded-md border border-neutral-700 bg-neutral-900 p-1 shadow-xl">
          <div className="flex items-center justify-between px-2 py-1 text-[10px] uppercase tracking-wider text-neutral-500">
            <span>Show branches</span>
            <div className="flex gap-2">
              <button
                onClick={() => onChange(null)}
                className="text-indigo-300 hover:underline"
              >
                All
              </button>
              <button
                onClick={() => onChange([])}
                className="text-neutral-400 hover:underline"
              >
                None
              </button>
            </div>
          </div>
          <div className="max-h-72 overflow-auto">
            {branches.map((b) => {
              const on = selected.has(b);
              return (
                <button
                  key={b}
                  onClick={() => toggle(b)}
                  className="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs hover:bg-neutral-800"
                >
                  <span
                    className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
                      on ? "border-indigo-500 bg-indigo-600" : "border-neutral-600"
                    }`}
                  >
                    {on && <Check className="h-2.5 w-2.5 text-white" />}
                  </span>
                  <span className="truncate font-mono text-neutral-200">{b}</span>
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
