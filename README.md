# gitui — a personal, free stacked-PR tool

A lightweight desktop app (a personal "Graphite") to manage **stacks of branches**:
visualize the branch tree, create/track/re-parent branches, **restack** (cascading
rebase) when a parent moves, and **submit** — push and open/update GitHub PRs in the
right order, each based on its parent.

Built with **Tauri v2** (Rust core) + **React + TypeScript + Tailwind**. The Rust core
shells out to your system `git` and the GitHub CLI (`gh`), so it reuses your existing
git behaviour and `gh` authentication — no tokens to manage.

## Prerequisites

- **Node.js** + npm
- **Rust** (stable) — for the Tauri backend
- **git** on `PATH`
- **GitHub CLI (`gh`)**, authenticated: `gh auth login` (needed for PR features)
- **Windows**: the Edge **WebView2** runtime (preinstalled on Windows 11) and MSVC build tools

## Develop & build

```bash
npm install
npm run tauri dev      # run the app (hot-reloads frontend, rebuilds Rust on change)
npm run tauri build    # produce a bundled desktop app

cd src-tauri && cargo test   # run the engine tests (git operations on temp repos)
```

## How it works

### Stack metadata
Each tracked branch records, in the repo's **git config**, its place in the stack:

| key | meaning |
| --- | --- |
| `branch.<name>.gitstack-parent` | the branch it is stacked on |
| `branch.<name>.gitstack-base`   | the parent SHA at the last sync (the `--onto` anchor) |
| `branch.<name>.gitstack-pr`     | the associated PR number |

Git-native, survives normal git operations, and inspectable with `git config --get-regexp gitstack`.

### Restack (cascading rebase)
For each branch in topological order (parents first):

```sh
git rebase --onto <parent's current tip> <recorded base SHA> <branch>
```

Using the **recorded base** as `<oldbase>` is what stops commits from being duplicated
or dropped. On conflict the rebase pauses; the app surfaces the conflicted files and
offers **Continue** (after you resolve + `git add`) or **Abort**.

### Submit
Bottom-up, for each branch:

```sh
git push --force-with-lease --force-if-includes -u origin <branch>
gh pr create --head <branch> --base <parent>   # or: gh pr edit <n> --base <parent>
```

Each PR's base points at its **parent branch** (the trunk for the bottom of the stack).

## v1 scope

Single repository, end to end: branch tree → create/track/re-parent → restack (with
conflict handling) → submit. The trunk (main/master) is auto-detected, never hardcoded.

## Roadmap

- View **multiple repositories** at once
- **Inter-repo dependency graph** (links discovered from `package.json` etc.)
- Auto re-parenting of children after a parent is **squash-merged**
- Generated Rust↔TS types (`ts-rs`)
- A headless **CLI** reusing the same core
- Packaging / installer

## Project layout

```
src/                     React + Vite frontend
  components/            StackTree, BranchRow, PrBadge, ConflictPanel, dialogs
  lib/                   typed invoke() wrappers + types mirroring the Rust model
src-tauri/src/
  commands.rs            #[tauri::command] handlers (+ engine tests)
  git/                   git subprocess wrapper (CREATE_NO_WINDOW on Windows)
  github/                gh CLI wrapper + pure submit planner
  meta/                  per-branch metadata via git config
  stack/                 topological order + restack engine
  model.rs  error.rs  proc.rs
```
