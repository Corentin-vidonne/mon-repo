# Sécurité — gitui

Suivi du durcissement suite à l'audit du **2026-06-05**.
Rapport complet et findings bruts : [`security-report/report.md`](security-report/report.md).

> Modèle de menace : gitui ingère des **données non fiables** (dépôts clonés, PR, sorties
> `git`/`gh`) et pilote un **LLM avec accès shell**. Le risque principal n'est pas
> l'injection classique mais l'escalade « j'ouvre un dépôt/PR hostile → exécution de code ».

## Correctifs appliqués

### P0
- **CSP stricte** — `src-tauri/tauri.conf.json` (était `csp: null`). Bloque l'exécution de
  script arbitraire dans la webview (multiplicateur de toute future XSS → IPC/RCE).
- **Validation de l'hôte Ollama** (`assist::validate_ollama_host`) — refus des schémas non
  http(s) et des cibles SSRF/métadonnées (lien-local `169.254.0.0/16`, `0.0.0.0`). Appliquée
  dans `set_ai_config` (jamais stocker un hôte dangereux), `ai_env` (re-validation) et
  `ollama_models` (le sink `GET /api/tags`). `ureq` reste sans TLS (l'hôte normal est
  loopback ; le cloud Ollama est relayé par le daemon local) — la validation est le contrôle.
- **Approbation LLM mono-tour** — `src/components/ChatDock.tsx` : une autorisation n'est plus
  persistée pour toute la session ; les commandes composées (`&&`, `;`, `|`, …) ne sont plus
  auto-approuvées (le modèle doit les exécuter une par une, chacune ré-approuvée).
- **Pipeline de release** — `.github/workflows/release.yml` : toutes les actions épinglées sur
  SHA de commit ; `permissions: contents: read` par défaut ; `.github/dependabot.yml` ajouté.

### P1
- **Allowlist git « lecture seule » durcie** — `assist::ai_env` injecte un env (via
  `GIT_CONFIG_*`, prioritaire sur la config du dépôt) qui bloque le transport `ext::` (un
  `.gitmodules` hostile pourrait sinon lancer une commande) et le pager, pour tout `git`
  lancé par `claude`. Le wrapper central `git::git()` applique le même durcissement à nos
  propres lectures sur les dépôts clonés. (Le diff externe/textconv n'est pas atteignable via
  un clone : il exige une config locale qu'un clone ne transporte pas.)
- **Injection d'arguments git** — `clone_repo` refuse une URL commençant par `-`, passe `--`
  et `-c protocol.ext.allow=never`.
- **URLs externes** — helper central `src/lib/safeOpen.ts` (http(s) uniquement) ; tous les
  `openUrl` y passent. Capability `opener` réduite à `allow-open-url`.

## À finaliser : signature des binaires (P1-5) — nécessite tes certificats

Non automatisable sans secrets. Tant que les installeurs ne sont diffusés qu'à toi-même, ce
point est repoussable ; il devient nécessaire pour toute **diffusion publique**.

Quand tu auras les certificats, `tauri-action` les lit automatiquement :

**macOS** (Developer ID + notarisation) — dans `tauri.conf.json` :
```jsonc
"bundle": { "macOS": { "signingIdentity": "Developer ID Application: …", "hardenedRuntime": true } }
```
Secrets CI : `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.

**Windows** (Authenticode) — dans `tauri.conf.json` :
```jsonc
"bundle": { "windows": { "certificateThumbprint": "…" } }   // ou "signCommand" (HSM/cloud)
```

Si un auto-updater est ajouté : configurer la clé publique minisign de l'updater Tauri (clé
privée uniquement en secret CI).

## Invariants à préserver

- **Pas de `rehype-raw` ni de `dangerouslySetInnerHTML`** dans le rendu Markdown : c'est la
  raison pour laquelle aucun sink XSS n'existe aujourd'hui (DAST-010).
- Tout nouvel `openUrl` passe par **`safeOpen`** (`src/lib/safeOpen.ts`).
- Tout sous-process passe par **`proc::`** (jamais de shell).
- Toute lecture git passe par **`git::git()`** (env durci) ; tout nouvel hôte réseau saisi par
  l'utilisateur est validé.

## Backlog (P2)

Voir `security-report/report.md` § P2 : `dagre`→`@dagrejs/dagre`, advisories Rust *Linux-only*
(gtk3/glib/proc-macro-error, transitifs), et **ajouter `cargo-audit`/`npm audit` à la CI**
(le pass Rust de l'audit était un recoupement RUSTSEC manuel, pas une requête live).
