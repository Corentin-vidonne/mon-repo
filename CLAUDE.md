# CLAUDE.md — gitui

Personal, free **stacked-PR tool** (a self-hosted "Graphite"): visualize a branch
tree, create/track/re-parent branches, **restack** (cascading rebase), and **submit**
(push + open/update GitHub PRs). Built with **Tauri v2** (Rust core) + **React + TS +
Tailwind**. The Rust core shells out to the system `git` and the GitHub CLI (`gh`).

## Testing — use the sandbox repo

For ANY test that runs real `git` / `gh` / `claude` against a working repo, use:

```
C:\Users\coren\Documents\projet\gitui-sandbox
```

It's a throwaway git repo (branches: `main`, `feat-a`, `feat-b`, `ai-review-demo`, with
an `origin` remote) meant exactly for this. **Never** run experiments against the gitui
repo itself. Feel free to create/delete branches, commits and PRs there.

## Build & test

```bash
npm install
npm run tauri dev            # run the app (hot-reload frontend, rebuild Rust on change)
npm run build                # tsc + vite build (frontend type-check + bundle)
npx tsc --noEmit             # frontend type-check only

cargo test --manifest-path src-tauri/Cargo.toml --lib   # Rust unit tests
```

Tests marked `#[ignore]` hit the real `claude` CLI / `gh` and the sandbox — run them
explicitly, e.g. `cargo test --lib <name> -- --ignored --nocapture`.

## Architecture

- `src-tauri/src/`
  - `commands.rs` — `#[tauri::command]` handlers (+ most engine tests)
  - `git/`, `github/` — subprocess wrappers (`git`, `gh`); `CREATE_NO_WINDOW` on Windows
  - `stack/` — topological order + restack engine; `cleanup_merged` re-parents children
  - `meta/` — per-branch stack metadata in git config (`branch.<name>.gitstack-*`)
  - `assist/` — builds the prompts fed to `claude` + how it's launched
  - `term/` — embedded PTY sessions that run `claude` interactively (xterm dock)
  - `chat/` — headless streaming `claude` chat sessions (the ChatDock)
  - `undo/` — per-repo snapshot stack powering the toolbar "Undo"
- `src/` — React frontend; `lib/api.ts` mirrors the Rust commands, `lib/types.ts` the model

## The AI "aides" (Claude Code integration)

Three pipelines, all shelling out to the `claude` CLI. The read-only aides (Summary /
Detailed) and the merge assists choose between #1 and #3 via the `assistantUi` setting
("terminal" vs "chat", default **chat**).

1. **Interactive (PTY / terminal)** — `term/*` + `assist::pty_command`, streamed to the
   `TerminalDock` (xterm). Free text, multi-turn. Writing commands are NOT pre-allowed,
   so `claude` asks for confirmation in the terminal.
2. **Headless one-shot JSON** — `assist::run_claude_headless` (`claude -p`) +
   `extract_json`, parsed into typed structs. Used by **AI Review** (`review_pr`) and
   **conflict resolution** (`suggest_conflict_resolution`).
3. **Headless streaming chat** — `chat/*` runs `claude -p --output-format stream-json`
   (one process per turn; the frontend keeps `session_id` and resumes with `--resume`),
   rendered as bubbles in `ChatDock`. `--include-partial-messages` toggles the typewriter
   effect (`chatStreaming`). For **merges in chat**, only read-only tools are allowed, so a
   write is denied and surfaced in the `result` event's `permission_denials`; the UI shows
   an approval modal and, on OK, resumes with that command added to `--allowedTools`
   (per-action approval, derived per sub-command for compound `A && B` commands).

`assist::READONLY_TOOLS` is the pre-allowed read-only allowlist (`git show/log/diff/status`,
`gh pr view/diff/checks/list`, `Read`/`Grep`/`Glob`). `--allowedTools` is variadic, so the
positional prompt is passed after a `--` separator.

**AI backend (Anthropic vs Ollama).** All three pipelines run the same `claude` CLI; the
`aiBackend` setting picks the *engine* behind it. `assist::ai_config()` is a process-global
(`OnceLock<Mutex<…>>`, like `undo::global()`) synced from the frontend via `set_ai_backend`.
`assist::ai_env()` returns the env vars injected at every spawn: **Anthropic** sets only
`ANTHROPIC_MODEL` when a model is chosen (alias `sonnet`/`opus`/`haiku` or full name; empty =
account default, no env), leaving the small/fast model alone; **Ollama** sets
`ANTHROPIC_BASE_URL=<host>` + `ANTHROPIC_AUTH_TOKEN=ollama` + empty `ANTHROPIC_API_KEY` +
`ANTHROPIC_MODEL`/`ANTHROPIC_SMALL_FAST_MODEL=<model>` (the small model must also be local or
background calls fail). `claude` is still required in both modes —
it's only the *model* that becomes local (or an Ollama **cloud** model). Models are
auto-detected by `ollama_models`, which merges `~/.ollama/config.json` →
`integrations.claude.models` (what `ollama launch claude` configures — covers `*:cloud`
models that `/api/tags` omits) with `GET <host>/api/tags` (local, via the tiny `ureq` client).

## Conventions

- French UI copy and French `claude` prompts (the app's audience is French-speaking).
- New assists follow the existing pattern: a prompt builder in `assist/`, a command in
  `term/` (interactive) or `commands.rs` (headless), registration in `lib.rs`, an api.ts
  wrapper / direct `invoke`, and a button wired through the relevant panel.
- **Cross-platform** (Windows / macOS / Linux): every subprocess goes through `proc::`
  (never a shell). `proc::fix_path_env()` runs once at startup (`lib.rs`) so GUI launches
  on macOS/Linux find `git`/`gh`/`claude` despite not inheriting the login-shell `PATH`
  (no-op on Windows). The `health` command reports `git`/`gh` + `gh` auth; the frontend
  `DependencyGate` pops on launch with install links when one is missing. **Claude Code is
  checked at point of use**, not at startup: `assist::ensure_claude_available()` guards the
  three AI funnels (`run_claude_headless`, `pty_command`, `chat::spawn_turn`) and returns a
  friendly install message if `claude` is absent. Installers are built per-OS by
  `.github/workflows/release.yml` (tauri-action), plus a native Arch/EndeavourOS
  `.pkg.tar.zst` built in an `archlinux` container via `makepkg` + `packaging/arch/PKGBUILD`
  (wraps the prebuilt `--no-bundle` binary; attached to the same draft release with `gh`).
