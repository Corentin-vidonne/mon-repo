import type { StackNode, Branch, BranchActionKind } from "../lib/types";
import { BranchRow } from "./BranchRow";

type OnAction = (kind: BranchActionKind, branch: Branch) => void;
type OnSelect = (branch: Branch) => void;

function Node({
  node,
  onAction,
  onSelect,
  selected,
}: {
  node: StackNode;
  onAction?: OnAction;
  onSelect?: OnSelect;
  selected?: string | null;
}) {
  return (
    <div>
      <BranchRow
        branch={node.branch}
        onAction={onAction}
        onSelect={onSelect}
        isSelected={node.branch.name === selected}
      />
      {node.children.length > 0 && (
        <div className="ml-3 border-l border-neutral-800 pl-3">
          {node.children.map((c) => (
            <Node
              key={c.branch.name}
              node={c}
              onAction={onAction}
              onSelect={onSelect}
              selected={selected}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function StackTree({
  roots,
  onAction,
  onSelect,
  selected,
}: {
  roots: StackNode[];
  onAction?: OnAction;
  onSelect?: OnSelect;
  selected?: string | null;
}) {
  return (
    <div className="space-y-0.5">
      {roots.map((r) => (
        <Node
          key={r.branch.name}
          node={r}
          onAction={onAction}
          onSelect={onSelect}
          selected={selected}
        />
      ))}
    </div>
  );
}
