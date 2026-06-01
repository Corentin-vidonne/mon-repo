import { useState } from "react";
import { Modal } from "./Modal";

const inputClass =
  "w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-indigo-600";

export function NewBranchDialog({
  parent,
  branches,
  onSubmit,
  onClose,
}: {
  parent: string;
  branches: string[];
  onSubmit: (name: string, parent: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [par, setPar] = useState(parent);

  return (
    <Modal title="New branch" onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) onSubmit(name.trim(), par);
        }}
        className="space-y-3"
      >
        <div>
          <label className="mb-1 block text-xs text-neutral-400">Branch name</label>
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="feat/my-change"
            className={inputClass}
          />
        </div>
        <div>
          <label className="mb-1 block text-xs text-neutral-400">Parent (base)</label>
          <select value={par} onChange={(e) => setPar(e.target.value)} className={inputClass}>
            {branches.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
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
            type="submit"
            disabled={!name.trim()}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function SetParentDialog({
  branch,
  current,
  branches,
  onSubmit,
  onClose,
}: {
  branch: string;
  current: string | null;
  branches: string[];
  onSubmit: (parent: string) => void;
  onClose: () => void;
}) {
  const options = branches.filter((b) => b !== branch);
  const [par, setPar] = useState(
    current && options.includes(current) ? current : options[0] ?? ""
  );

  return (
    <Modal title={`Set parent of ${branch}`} onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (par) onSubmit(par);
        }}
        className="space-y-3"
      >
        <div>
          <label className="mb-1 block text-xs text-neutral-400">Parent branch</label>
          <select
            autoFocus
            value={par}
            onChange={(e) => setPar(e.target.value)}
            className={inputClass}
          >
            {options.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
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
            type="submit"
            disabled={!par}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            Save
          </button>
        </div>
      </form>
    </Modal>
  );
}
