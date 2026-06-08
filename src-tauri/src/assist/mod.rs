use crate::error::{AppError, Result};
use crate::git;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Resolve the full path to the `claude` executable (so we spawn it directly,
/// bypassing any shell — Windows PowerShell 5.1 mangles quoted/multi-line args).
pub(crate) fn resolve_claude() -> String {
    #[cfg(windows)]
    {
        if let Ok(r) = crate::proc::run("where", ["claude"], None) {
            if r.success {
                // Prefer a real .exe over a .cmd/.ps1 shim.
                if let Some(exe) = r.stdout.lines().find(|l| l.trim().ends_with(".exe")) {
                    return exe.trim().to_string();
                }
                if let Some(first) = r.stdout.lines().next() {
                    let f = first.trim();
                    if !f.is_empty() {
                        return f.to_string();
                    }
                }
            }
        }
    }
    "claude".to_string()
}

/// Shown when an AI feature is invoked but the `claude` CLI isn't installed. Includes the
/// install command + docs link. Claude is checked at point of use, never at startup.
pub(crate) const CLAUDE_MISSING_MSG: &str = "Claude Code introuvable. Installe la CLI `claude` \
     pour les aides IA : npm install -g @anthropic-ai/claude-code  ·  \
     https://docs.claude.com/en/docs/claude-code/setup";

/// Verify the `claude` CLI is runnable, returning a friendly install message if not. Called
/// at the entry of each AI funnel (headless / chat / terminal) so the dependency is only
/// checked when an AI feature is actually used. In Ollama mode `claude` is still the engine
/// (Ollama only supplies the model), so we also require that a model has been chosen.
pub(crate) fn ensure_claude_available() -> Result<()> {
    let claude = resolve_claude();
    let ok = crate::proc::run(&claude, ["--version"], None)
        .map(|r| r.success)
        .unwrap_or(false);
    if !ok {
        return Err(AppError::new(CLAUDE_MISSING_MSG));
    }
    let cfg = ai_config().lock().unwrap();
    if cfg.backend == AiBackend::Ollama && cfg.ollama_model.trim().is_empty() {
        return Err(AppError::new(
            "Mode Ollama actif mais aucun modèle choisi — sélectionnes-en un dans \
             Réglages → Backend IA.",
        ));
    }
    Ok(())
}

/// Which engine backs the `claude` CLI: Anthropic's cloud API (the user's own login) or a
/// local Ollama server exposing the Anthropic-compatible API.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AiBackend {
    Anthropic,
    Ollama,
}

#[derive(Clone, Debug)]
pub(crate) struct AiConfig {
    pub backend: AiBackend,
    pub ollama_host: String,
    pub ollama_model: String,
    /// Model for Anthropic mode (alias like `sonnet`/`opus`/`haiku` or a full name);
    /// empty means "use Claude Code's own default".
    pub anthropic_model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            backend: AiBackend::Anthropic,
            ollama_host: "http://localhost:11434".to_string(),
            ollama_model: String::new(),
            anthropic_model: String::new(),
        }
    }
}

/// Process-global AI backend config (same pattern as `undo::global()`), so the spawn
/// funnels — which are plain functions, not command handlers with `State` — can read it
/// without threading it through every call site. Synced from the frontend settings.
pub(crate) fn ai_config() -> &'static Mutex<AiConfig> {
    static CFG: OnceLock<Mutex<AiConfig>> = OnceLock::new();
    CFG.get_or_init(|| Mutex::new(AiConfig::default()))
}

/// Default Ollama endpoint (loopback), used when none or an invalid host is configured.
pub(crate) const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";

/// Validate & normalize a user-supplied Ollama host before it is queried (`ollama_models`)
/// or injected as `ANTHROPIC_BASE_URL` into every spawned `claude`. Rejects non-http(s)
/// schemes and SSRF / cloud-metadata targets (link-local `169.254.0.0/16`, `0.0.0.0`,
/// broadcast). Loopback, LAN and public hosts are allowed — the user may legitimately run
/// Ollama elsewhere — so this denies the dangerous ranges rather than locking to loopback.
pub(crate) fn validate_ollama_host(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_OLLAMA_HOST.to_string());
    }
    let u = url::Url::parse(raw)
        .map_err(|_| AppError::new("Hôte Ollama invalide (URL malformée)."))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(AppError::new("Hôte Ollama : seuls les schémas http/https sont autorisés."));
    }
    let host = u
        .host_str()
        .ok_or_else(|| AppError::new("Hôte Ollama : nom d'hôte manquant."))?;
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast()
            }
            std::net::IpAddr::V6(v6) => v6.is_unspecified(),
        };
        if blocked {
            return Err(AppError::new(
                "Hôte Ollama : adresse non autorisée (lien-local / métadonnées cloud).",
            ));
        }
    }
    Ok(u.as_str().trim_end_matches('/').to_string())
}

/// Update the global config from the frontend (`anthropic` or `ollama`).
pub(crate) fn set_ai_config(
    backend: &str,
    ollama_host: String,
    ollama_model: String,
    anthropic_model: String,
) {
    let mut cfg = ai_config().lock().unwrap();
    cfg.backend = if backend.eq_ignore_ascii_case("ollama") {
        AiBackend::Ollama
    } else {
        AiBackend::Anthropic
    };
    // Never store a dangerous host: an invalid value falls back to loopback so it can't be
    // used as an SSRF target or a plaintext-exfiltration sink (see `validate_ollama_host`).
    cfg.ollama_host =
        validate_ollama_host(&ollama_host).unwrap_or_else(|_| DEFAULT_OLLAMA_HOST.to_string());
    cfg.ollama_model = ollama_model;
    cfg.anthropic_model = anthropic_model;
}

/// Environment variables to inject when launching `claude` (applied by all three funnels).
/// Always includes git-hardening vars: `claude` reads untrusted repos/PRs, and its pre-
/// allowed "read-only" git tools (`git show/log/diff`) can otherwise be steered — e.g. an
/// `ext::` URL in a hostile `.gitmodules` — into running a command. `GIT_CONFIG_*` has higher
/// precedence than the repo's own config, so a malicious cloned repo can't re-enable these.
/// On top of that, Ollama mode points `claude` at the (validated) local server and pins both
/// the main and the small/fast model — otherwise Claude Code's background calls would try
/// (and fail) to reach Anthropic. Anthropic mode adds only `ANTHROPIC_MODEL` when chosen.
pub(crate) fn ai_env() -> Vec<(String, String)> {
    let cfg = ai_config().lock().unwrap().clone();
    let mut env = vec![
        ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
        ("GIT_CONFIG_COUNT".to_string(), "2".to_string()),
        ("GIT_CONFIG_KEY_0".to_string(), "core.pager".to_string()),
        ("GIT_CONFIG_VALUE_0".to_string(), "cat".to_string()),
        ("GIT_CONFIG_KEY_1".to_string(), "protocol.ext.allow".to_string()),
        ("GIT_CONFIG_VALUE_1".to_string(), "never".to_string()),
    ];
    match cfg.backend {
        AiBackend::Anthropic => {
            // Override only the main model when chosen; leave the small/fast model as the
            // account default (no reason to pay big-model cost for background calls). Empty
            // means Claude Code's own default model.
            let model = cfg.anthropic_model.trim();
            if !model.is_empty() {
                env.push(("ANTHROPIC_MODEL".to_string(), model.to_string()));
            }
        }
        AiBackend::Ollama => {
            // Re-validate defensively (set_ai_config already stores only safe hosts).
            let host = validate_ollama_host(&cfg.ollama_host)
                .unwrap_or_else(|_| DEFAULT_OLLAMA_HOST.to_string());
            env.push(("ANTHROPIC_BASE_URL".to_string(), host));
            env.push(("ANTHROPIC_AUTH_TOKEN".to_string(), "ollama".to_string()));
            env.push(("ANTHROPIC_API_KEY".to_string(), String::new()));
            let model = cfg.ollama_model.trim();
            if !model.is_empty() {
                env.push(("ANTHROPIC_MODEL".to_string(), model.to_string()));
                env.push(("ANTHROPIC_SMALL_FAST_MODEL".to_string(), model.to_string()));
            }
        }
    }
    env
}

/// The prompt injected into `claude` to analyze a commit.
/// `mode` is "summary" (short synthesis) or "detailed" (in-depth review).
pub fn analysis_prompt(repo: &Path, sha: &str, mode: &str) -> Result<String> {
    let detail = git::commit_detail(repo, sha)?;
    let subject = detail
        .message
        .lines()
        .next()
        .unwrap_or("")
        .replace('"', "'");
    let files: Vec<String> = detail.files.iter().map(|f| f.path.clone()).collect();
    let short: String = sha.chars().take(8).collect();
    let files_line = if files.is_empty() {
        "(aucun)".to_string()
    } else {
        files.join(", ")
    };

    let body = if mode == "summary" {
        format!(
            "Donne un RÉSUMÉ SYNTHÉTIQUE (5 à 8 lignes maximum) :\n\
             - ce que fait ce commit, en une phrase ;\n\
             - les changements clés, fichier par fichier ;\n\
             - l'intention probable derrière le changement.\n\
             Va à l'essentiel."
        )
    } else {
        format!(
            "Fournis une ANALYSE COMPLÈTE et structurée :\n\
             1. Résumé : ce que fait ce commit.\n\
             2. Détail par fichier / fonction : ce qui change et pourquoi.\n\
             3. Intention et conception : le but, les choix de design.\n\
             4. Impact : effets sur le reste du code, compatibilité, performances.\n\
             5. Risques et bugs potentiels : points fragiles, cas limites non gérés.\n\
             6. Suggestions : améliorations possibles et tests à ajouter.\n\
             Sois précis et cite le code concerné."
        )
    };

    Ok(format!(
        "Tu es un relecteur de code expert. Analyse le commit `{short}` (sujet : {subject}) de ce dépôt git.\n\n\
         Commence par exécuter `git show {sha}` pour lire le diff complet (explore les fichiers concernés si besoin).\n\n\
         {body}\n\n\
         Fichiers modifiés : {files_line}.\n\n\
         Ensuite, reste disponible : je vais te poser des questions sur ce code.",
    ))
}

/// The prompt injected into `claude` to analyze a whole Pull Request.
/// `mode` is "summary" or "detailed".
pub fn pr_analysis_prompt(
    number: u64,
    title: &str,
    head: &str,
    base: &str,
    mode: &str,
) -> String {
    let title = title.replace('"', "'");
    let body = if mode == "summary" {
        "Donne un RÉSUMÉ SYNTHÉTIQUE (5 à 8 lignes maximum) :\n\
         - l'objectif de la PR, en une phrase ;\n\
         - les changements clés, regroupés par thème ;\n\
         - tout point qui mérite l'attention du relecteur.\n\
         Va à l'essentiel."
    } else {
        "Fournis une RELECTURE DE PR COMPLÈTE et structurée :\n\
         1. Objectif : le problème résolu et l'approche.\n\
         2. Tour des changements : par fichier / module, ce qui change et pourquoi.\n\
         3. Qualité & conception : lisibilité, choix d'architecture, cohérence.\n\
         4. Risques & bugs potentiels : cas limites, régressions, sécurité.\n\
         5. Tests : couverture, ce qu'il manque.\n\
         6. Verdict : prêt à merger ? sinon, les points bloquants.\n\
         Sois précis et cite le code concerné."
    };
    format!(
        "Tu es un relecteur de code expert. Analyse la Pull Request #{number} (titre : {title}) de ce dépôt.\n\n\
         Commence par exécuter `gh pr view {number}` (description) puis `gh pr diff {number}` (diff complet) ; explore les fichiers concernés si besoin.\n\n\
         {body}\n\n\
         Branche : `{head}` → `{base}`.\n\n\
         Ensuite, reste disponible : je vais te poser des questions sur cette PR.",
    )
}

/// The prompt injected into `claude` to ASSIST with merging a Pull Request, in the
/// context of this stacked-PR tool: check readiness, choose a strategy, run the merge
/// (only after the user confirms), then re-sync the stack. Unlike the analysis prompts
/// this one is meant to *act* — `gh pr merge` is NOT among the pre-allowed read-only
/// tools, so it will ask for confirmation in the terminal before anything lands.
pub fn merge_assist_prompt(number: u64, title: &str, head: &str, base: &str, trunk: &str) -> String {
    let title = title.replace('"', "'");
    let position = if base == trunk {
        format!(
            "Cette PR est à la BASE de la pile (sa base `{base}` est le tronc `{trunk}`) : \
             elle peut être mergée maintenant."
        )
    } else {
        format!(
            "ATTENTION : la base de cette PR est `{base}`, et non le tronc `{trunk}`. Dans une pile \
             on merge de bas en haut — la ou les PR parentes doivent être mergées d'abord. Signale-le \
             clairement et n'effectue PAS le merge tant que cette PR n'est pas posée sur le tronc."
        )
    };
    format!(
        "Tu es un expert Git/GitHub qui m'aide à MERGER une Pull Request dans un dépôt géré en PILES \
         de branches (stacked PRs). PR : #{number} (titre : {title}), branche `{head}` → `{base}`. \
         Tronc : `{trunk}`.\n\n\
         {position}\n\n\
         Avance par étapes et DEMANDE-MOI confirmation avant toute action qui écrit (le merge) :\n\
         1. Diagnostic de mergeabilité : exécute `gh pr view {number}` (état, reviewDecision, mergeable, \
            conflits) et `gh pr checks {number}` (CI). Résume en quelques lignes et liste clairement les \
            éventuels bloquants.\n\
         2. Stratégie : si tout est au vert, recommande une méthode. Par défaut pour une pile, `--squash` \
            (tronc linéaire ; les enfants seront re-parentés ensuite). Explique brièvement et laisse-moi \
            trancher.\n\
         3. Merge : après MON accord explicite, lance le merge, p. ex. `gh pr merge {number} --squash`. \
            N'ajoute PAS `--delete-branch` si des PR enfants sont encore empilées sur `{head}`.\n\
         4. Après le merge : rappelle-moi de cliquer sur **Sync** dans gitui — l'app fast-forward le tronc, \
            re-parente automatiquement les enfants de la branche mergée sur son parent et cesse de la suivre \
            (la branche locale n'est jamais supprimée).\n\n\
         Commence par l'étape 1, puis attends mes réponses ; reste disponible pour la suite."
    )
}

/// The prompt injected into `claude` to ASSIST with merging one local branch into
/// another (a plain `git merge`, NOT a PR). `source` is merged into `target`. As with
/// the PR merge assist, the writing commands (`git switch`/`merge`/`commit`) are not
/// pre-allowed, so they prompt for confirmation in the terminal.
pub fn branch_merge_prompt(source: &str, target: &str, trunk: &str) -> String {
    format!(
        "Tu es un expert Git qui m'aide à MERGER localement la branche `{source}` (source) dans la \
         branche `{target}` (cible). Le dépôt est géré en piles de branches (stacked PRs), tronc `{trunk}` ; \
         on y restacke (rebase) d'habitude, mais ici je veux EXPLICITEMENT un merge — respecte ce choix.\n\n\
         Avance par étapes et DEMANDE-MOI confirmation avant toute commande qui écrit \
         (checkout, merge, commit) :\n\
         1. État des lieux : `git status` (l'arbre de travail est-il propre ?), puis compare les branches — \
            `git log --oneline {target}..{source}` (ce que `{source}` apporte) et `git log --oneline {source}..{target}` \
            (ce qui manque à `{source}`). Indique si un fast-forward est possible et signale les risques de conflit.\n\
         2. Stratégie : recommande la méthode adaptée — fast-forward si possible, sinon un commit de merge \
            (`git merge {source}`), ou `--squash` si je veux un seul commit. Explique brièvement et laisse-moi trancher.\n\
         3. Exécution : après MON accord, place-toi sur la cible (`git switch {target}`) puis lance le merge \
            (p. ex. `git merge {source}`). Montre le résultat.\n\
         4. En cas de conflit : NE devine pas — liste les fichiers en conflit (`git status`), aide-moi à les \
            résoudre un par un (tu peux proposer le contenu final de chaque fichier), puis finalise avec \
            `git add` + `git commit`. Si je préfère annuler, utilise `git merge --abort`.\n\n\
         Commence par l'étape 1 et attends mes réponses ; reste disponible pour la suite."
    )
}

/// Prompt asking `claude` to write a commit message for `sha`, returned as JSON.
/// `mode` is "simple" (≤5 words) or "complet" (subject + body). The message must start
/// with a conventional-commit type (`feat:`, `fix:`, `update:`, …).
pub fn commit_message_prompt(sha: &str, mode: &str) -> String {
    let short: String = sha.chars().take(8).collect();
    let spec = if mode == "simple" {
        "Génère un message TRÈS COURT : le préfixe conventionnel suivi de 5 MOTS MAXIMUM \
         (ex. `fix: corrige le crash au démarrage`). Une seule ligne, aucun corps."
    } else {
        "Génère un message COMPLET : une ligne de sujet (préfixe conventionnel, ~50 caractères) \
         qui résume le changement, puis une ligne vide, puis un corps en quelques puces \
         expliquant le quoi et le pourquoi."
    };
    format!(
        "Tu es un expert Git. Lis le commit `{short}` avec `git show {sha}` (diff complet), \
         puis rédige SON message de commit.\n\n\
         Le message DOIT commencer par un type conventionnel suivi de deux-points — l'un de : \
         `feat:`, `fix:`, `update:`, `refactor:`, `docs:`, `test:`, `chore:`, `style:`, `perf:`, \
         `build:`, `ci:` — choisis le plus adapté au diff.\n\
         {spec}\n\n\
         Rédige en français. RÉPONDS UNIQUEMENT avec un objet JSON valide, sans Markdown, de la forme :\n\
         {{\"message\": \"<le message de commit, \\n autorisés pour le corps>\"}}"
    )
}

/// Prompt to REVIEW a single commit and return STRUCTURED JSON findings (same contract
/// as `pr_review_prompt`, but for one commit's diff).
pub fn commit_review_prompt(sha: &str, message: &str, diff: &str) -> String {
    let short: String = sha.chars().take(8).collect();
    let subject = message.lines().next().unwrap_or("").replace('"', "'");
    format!(
        "Tu es un relecteur de code expert et exigeant. Relis le commit `{short}` (sujet : {subject}).\n\
         Voici son DIFF UNIFIÉ COMPLET (il a pu être tronqué s'il est très volumineux) :\n\
         ```diff\n{diff}\n```\n\n\
         Analyse le diff et relève les problèmes concrets et actionnables : bugs, régressions, cas limites, \
         sécurité, fuites/performances, lisibilité, tests manquants.\n\n\
         RÉPONDS UNIQUEMENT avec un objet JSON valide — aucun texte avant ou après, pas de Markdown. \
         Les clés doivent être EXACTEMENT `summary` et `findings`. Forme :\n\
         {{\"summary\": \"<2 à 4 phrases : ce que fait le commit et le verdict global>\", \
         \"findings\": [{{\"file\": \"<chemin relatif>\", \"line\": <numéro ou null>, \
         \"severity\": \"info|warning|critical\", \"title\": \"<titre court>\", \"detail\": \"<explication + correctif>\"}}]}}\n\
         Limite-toi aux ~15 findings les plus importants ; si rien à signaler, renvoie une liste vide. \
         Rédige summary, title et detail en français."
    )
}

/// Prompt to suggest a branch name (kebab-case, conventional prefix) from a context blurb.
pub fn branch_name_prompt(context: &str) -> String {
    format!(
        "Propose UN nom de branche git, court, basé sur ce contexte :\n{context}\n\n\
         Règles : préfixe de type (`feat/`, `fix/`, `chore/`, `refactor/`, `docs/`, `test/`…), \
         puis 2 à 4 mots en kebab-case (minuscules, séparés par des tirets), sans espaces ni accents.\n\
         RÉPONDS UNIQUEMENT avec un objet JSON : {{\"name\": \"feat/mon-changement\"}}"
    )
}

/// Prompt to draft a PR title + Markdown body from a branch's commits and diffstat.
pub fn pr_description_prompt(branch: &str, base: &str, commits: &str, stat: &str) -> String {
    format!(
        "Tu rédiges la description d'une Pull Request pour la branche `{branch}` (base `{base}`).\n\
         Commits de la branche :\n{commits}\n\n\
         Fichiers modifiés (git diff --stat) :\n{stat}\n\n\
         Produis un TITRE concis (préfixe conventionnel `feat:`/`fix:`/… si pertinent, ~60 caractères) \
         et un CORPS en Markdown : 1 à 2 phrases de contexte, puis une liste à puces des changements clés, \
         et au besoin une courte section « Notes ». Rédige en français.\n\
         RÉPONDS UNIQUEMENT avec un objet JSON : {{\"title\": \"<titre>\", \"body\": \"<corps Markdown, \\n autorisés>\"}}"
    )
}

/// Run `claude` non-interactively (print mode) and return its stdout. Unlike the
/// PTY path (which streams free text to a terminal), this is for one-shot calls
/// whose output we parse as JSON. The needed context is embedded in `prompt`.
pub fn run_claude_headless(repo: &Path, prompt: &str) -> Result<String> {
    ensure_claude_available()?;
    let claude = resolve_claude();
    let mut args: Vec<String> = Vec::new();
    push_allowed_tools(&mut |a| args.push(a.to_string()));
    args.push("-p".to_string()); // print / non-interactive mode
    // `--` ends option parsing so the variadic `--allowedTools` can't swallow the prompt.
    args.push("--".to_string());
    args.push(prompt.to_string());
    // Backend env (empty for Anthropic; Ollama points claude at the local model).
    let env = ai_env();
    let env_ref: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let r = crate::proc::run_env(&claude, args.iter().map(String::as_str), Some(repo), &env_ref)
        .map_err(|e| AppError::new(format!("Could not run claude: {e}")))?;
    if !r.success {
        return Err(AppError::new(format!(
            "claude failed: {}",
            r.stderr.trim()
        )));
    }
    Ok(r.stdout)
}

/// Pull the first `{ ... }` object out of model output, tolerating any prose or
/// Markdown fences the model may wrap around it.
pub fn extract_json(s: &str) -> Result<&str> {
    let start = s
        .find('{')
        .ok_or_else(|| AppError::new("claude returned no JSON"))?;
    let end = s
        .rfind('}')
        .ok_or_else(|| AppError::new("claude returned no JSON"))?;
    if end > start {
        Ok(&s[start..=end])
    } else {
        Err(AppError::new("claude returned no JSON"))
    }
}

/// Prompt asking `claude` to review a whole PR and return STRUCTURED JSON findings.
pub fn pr_review_prompt(detail: &crate::model::PrDetail) -> String {
    let files: Vec<String> = detail.files.iter().map(|f| f.path.clone()).collect();
    let files_line = if files.is_empty() {
        "(aucun)".to_string()
    } else {
        files.join(", ")
    };
    let commits = if detail.commits.is_empty() {
        "(aucun)".to_string()
    } else {
        detail.commits.join("\n- ")
    };
    format!(
        "Tu es un relecteur de code expert et exigeant. Relis la Pull Request #{number} (titre : {title}).\n\
         Branche : `{head}` → `{base}`. Fichiers : {files_line}.\n\
         Commits :\n- {commits}\n\n\
         Voici le DIFF UNIFIÉ COMPLET de la PR (il a pu être tronqué s'il est très volumineux) :\n\
         ```diff\n{diff}\n```\n\n\
         Analyse le diff et relève les problèmes concrets et actionnables : bugs, régressions, cas limites, \
         sécurité, fuites/performances, lisibilité, tests manquants.\n\n\
         RÉPONDS UNIQUEMENT avec un objet JSON valide — aucun texte avant ou après, pas de balises Markdown. \
         Les clés JSON doivent être EXACTEMENT `summary` et `findings` (en anglais, pas `issues`). Forme :\n\
         {{\"summary\": \"<2 à 4 phrases : objectif de la PR et verdict global>\", \
         \"findings\": [{{\"file\": \"<chemin relatif>\", \"line\": <numéro de ligne ou null>, \
         \"severity\": \"info|warning|critical\", \"title\": \"<titre court>\", \"detail\": \"<explication + correctif suggéré>\"}}]}}\n\
         Limite-toi aux ~20 findings les plus importants ; si rien à signaler, renvoie une liste \"findings\" vide. \
         Rédige summary, title et detail en français.",
        number = detail.number,
        title = detail.title.replace('"', "'"),
        head = detail.head_ref,
        base = detail.base_ref,
        files_line = files_line,
        commits = commits,
        diff = detail.diff,
    )
}

/// Prompt asking `claude` to resolve a single conflicted file and return STRUCTURED JSON.
pub fn conflict_resolution_prompt(
    file: &str,
    marked: &str,
    base: Option<&str>,
    ours: Option<&str>,
    theirs: Option<&str>,
) -> String {
    let section = |label: &str, content: Option<&str>| -> String {
        match content {
            Some(c) => format!("\n--- {label} ---\n```\n{c}\n```\n"),
            None => String::new(),
        }
    };
    format!(
        "Tu es un expert Git. Le fichier `{file}` est en conflit de merge/rebase. \
         Voici son contenu actuel AVEC les marqueurs de conflit (<<<<<<<, =======, >>>>>>>) :\n\
         ```\n{marked}\n```\n{base}{ours}{theirs}\n\
         Résous le conflit en produisant le contenu FINAL et COMPLET du fichier, cohérent et compilable, \
         en combinant correctement les deux côtés et SANS aucun marqueur de conflit.\n\n\
         RÉPONDS UNIQUEMENT avec un objet JSON valide — aucun texte avant ou après, pas de Markdown — de la forme :\n\
         {{\"explanation\": \"<2 à 4 phrases en français : ce que tu as gardé de chaque côté et pourquoi>\", \
         \"resolution\": \"<contenu COMPLET du fichier résolu, sans marqueurs>\"}}",
        file = file,
        marked = marked,
        base = section("BASE (ancêtre commun)", base),
        ours = section("OURS (HEAD courant)", ours),
        theirs = section("THEIRS (commit appliqué)", theirs),
    )
}

/// Read-only tools pre-allowed so Claude can inspect a commit without prompting,
/// while STILL asking before anything that writes or runs arbitrary commands.
/// `--allowedTools` is VARIADIC (`<tools...>`): it greedily consumes every
/// following argument until the next flag. The positional prompt is therefore
/// passed after a `--` separator (see `pty_command`) so it isn't swallowed as a
/// tool value — otherwise `claude` launches with no prompt and nothing is sent.
const READONLY_TOOLS: [&str; 11] = [
    "Bash(git show:*)",
    "Bash(git log:*)",
    "Bash(git diff:*)",
    "Bash(git status:*)",
    "Bash(gh pr view:*)",
    "Bash(gh pr diff:*)",
    "Bash(gh pr checks:*)",
    "Bash(gh pr list:*)",
    "Read",
    "Grep",
    "Glob",
];

pub(crate) fn push_allowed_tools(push: &mut impl FnMut(&str)) {
    for t in READONLY_TOOLS {
        push("--allowedTools");
        push(t);
    }
}

/// A `CommandBuilder` that runs `claude` pre-seeded with `prompt`, for use inside a PTY.
/// Spawned directly (no shell) so the multi-line prompt arg is passed intact.
pub fn pty_command(repo: &Path, prompt: &str) -> Result<portable_pty::CommandBuilder> {
    ensure_claude_available()?;
    let mut cmd = portable_pty::CommandBuilder::new(resolve_claude());
    for (k, v) in ai_env() {
        cmd.env(k, v);
    }
    push_allowed_tools(&mut |a| {
        cmd.arg(a);
    });
    // `--` ends option parsing so the variadic `--allowedTools` above cannot
    // swallow the prompt; it is then taken as the positional `[prompt]` arg.
    cmd.arg("--");
    cmd.arg(prompt);
    cmd.cwd(repo);
    Ok(cmd)
}

/// Launch `claude` in a separate external terminal window (non-embedded fallback).
#[allow(dead_code)]
pub fn launch_claude(repo: &Path, prompt: &str) -> Result<()> {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(resolve_claude());
    push_allowed_tools(&mut |a| {
        cmd.arg(a);
    });
    // `--` so the variadic `--allowedTools` doesn't swallow the prompt.
    cmd.arg("--").arg(prompt).current_dir(repo);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }
    cmd.spawn()
        .map_err(|e| AppError::new(format!("Could not launch claude: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ollama_host_allows_local_and_blocks_ssrf() {
        // Empty falls back to the loopback default.
        assert_eq!(validate_ollama_host("").unwrap(), DEFAULT_OLLAMA_HOST);
        // Loopback, LAN and public hosts are allowed (trailing slash trimmed).
        assert_eq!(
            validate_ollama_host("http://localhost:11434/").unwrap(),
            "http://localhost:11434"
        );
        assert!(validate_ollama_host("http://192.168.1.50:11434").is_ok());
        assert!(validate_ollama_host("https://ollama.example.com").is_ok());
        // SSRF / cloud-metadata targets and bogus inputs are rejected.
        assert!(validate_ollama_host("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_ollama_host("http://0.0.0.0:11434").is_err());
        assert!(validate_ollama_host("file:///etc/passwd").is_err());
        assert!(validate_ollama_host("ftp://example.com").is_err());
        assert!(validate_ollama_host("not a url").is_err());
    }

    #[test]
    fn merge_prompt_adapts_to_stack_position() {
        // Bottom of the stack (base == trunk): mergeable now, no bottom-up warning.
        let bottom = merge_assist_prompt(7, "Ma feature", "feat/x", "main", "main");
        assert!(bottom.contains("#7"));
        assert!(bottom.contains("gh pr merge 7"));
        assert!(bottom.contains("BASE de la pile"));
        assert!(!bottom.contains("de bas en haut"));

        // Mid-stack (base is another branch): warn to merge the parent PR(s) first.
        let mid = merge_assist_prompt(7, "Ma feature", "feat/x", "feat/parent", "main");
        assert!(mid.contains("ATTENTION"));
        assert!(mid.contains("de bas en haut"));
    }

    #[test]
    fn branch_merge_prompt_names_both_branches_and_direction() {
        let p = branch_merge_prompt("feat/x", "main", "main");
        assert!(p.contains("`feat/x`"));
        assert!(p.contains("`main`"));
        // Direction: source merged into target, on the target branch.
        assert!(p.contains("git merge feat/x"));
        assert!(p.contains("git switch main"));
    }

    #[test]
    fn commit_message_prompt_enforces_prefix_and_mode() {
        let simple = commit_message_prompt("abc1234567", "simple");
        assert!(simple.contains("git show abc1234567"));
        assert!(simple.contains("feat:") && simple.contains("fix:") && simple.contains("update:"));
        assert!(simple.contains("5 MOTS MAXIMUM"));
        assert!(simple.contains("\"message\""));

        let complet = commit_message_prompt("abc1234567", "complet");
        assert!(complet.contains("COMPLET") && complet.contains("corps"));
    }

    // Regression: `--allowedTools` is variadic, so the prompt MUST be passed
    // after a `--` separator. Without it the prompt is consumed as a tool value
    // and `claude` starts with no input — "nothing is sent".
    #[test]
    fn prompt_passed_as_positional_after_double_dash() {
        let prompt = "Tu es un relecteur de code.\nAnalyse le commit `abc12345`.";
        let cmd = pty_command(Path::new("."), prompt).unwrap();
        let argv: Vec<&str> = cmd
            .get_argv()
            .iter()
            .map(|a| a.to_str().unwrap())
            .collect();

        // The prompt is the final argument...
        assert_eq!(*argv.last().unwrap(), prompt);
        // ...immediately preceded by a `--` separator...
        assert_eq!(argv[argv.len() - 2], "--");
        // ...that sits after every `--allowedTools` flag.
        let dd = argv.iter().rposition(|a| *a == "--").unwrap();
        let last_tools = argv.iter().rposition(|a| *a == "--allowedTools").unwrap();
        assert!(last_tools < dd, "-- must come after all --allowedTools flags");
    }

    // Ground-truth smoke test: spawns REAL `claude` interactively through a PTY
    // (exactly like the app) and checks the seeded prompt is auto-submitted.
    // Hits the API (one trivial turn). Run explicitly:
    //   cargo test --lib interactive_pty_autosubmits -- --ignored --nocapture
    #[test]
    #[ignore]
    fn interactive_pty_autosubmits() {
        use portable_pty::{native_pty_system, PtySize};
        use std::io::Read;
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        let repo = std::env::current_dir().unwrap();
        let prompt = "Ignore tout contexte. Réponds par un seul mot, exactement: PONGXYZ";
        let cmd = pty_command(&repo, prompt).unwrap();

        let pair = native_pty_system()
            .openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })
            .unwrap();
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();

        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let buf2 = buf.clone();
        let t = std::thread::spawn(move || {
            let mut tmp = [0u8; 8192];
            while let Ok(n) = reader.read(&mut tmp) {
                if n == 0 {
                    break;
                }
                buf2.lock().unwrap().extend_from_slice(&tmp[..n]);
            }
        });

        let deadline = Instant::now() + Duration::from_secs(40);
        let mut seen = false;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
            let s = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
            // "PONGXYZ" echoed in the assistant turn => the prompt was submitted.
            if s.matches("PONGXYZ").count() >= 2 {
                seen = true;
                break;
            }
        }
        let _ = child.kill();
        let _ = t.join();

        let out = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
        eprintln!(
            "----- PTY OUTPUT ({} bytes) -----\n{}\n----- END OUTPUT -----",
            out.len(),
            out
        );
        eprintln!("auto-submitted (PONGXYZ seen in a reply): {}", seen);
    }
}
