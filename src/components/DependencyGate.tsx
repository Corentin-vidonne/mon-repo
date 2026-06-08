import { useState } from "react";
import { AlertTriangle, Check, ExternalLink, RefreshCw, X } from "lucide-react";
import { safeOpen } from "../lib/safeOpen";
import type { Health } from "../lib/types";

type Tool = {
  name: string;
  /** Version string when installed, otherwise null. */
  version: string | null;
  /** What gitui uses it for (French). */
  desc: string;
  /** Where to install it. */
  url: string;
  /** A copy-pasteable install command shown when the tool is missing. */
  install: string;
};

function tools(h: Health): Tool[] {
  return [
    {
      name: "Git",
      version: h.gitVersion,
      desc: "Indispensable : toutes les opérations sur les branches, commits et la pile.",
      url: "https://git-scm.com/downloads",
      install: "winget install Git.Git · brew install git · apt install git",
    },
    {
      name: "GitHub CLI (gh)",
      version: h.ghVersion,
      desc: "Pull requests, issues, checks CI et « Submit ».",
      url: "https://cli.github.com/",
      install: "winget install GitHub.cli · brew install gh · apt install gh",
    },
  ];
}

/** Whether anything blocks normal use: a missing tool, or gh present but not logged in.
 * Claude Code is *not* part of this — it's verified only when an AI feature is used. */
export function dependenciesIncomplete(h: Health): boolean {
  return !h.gitVersion || !h.ghVersion || (!!h.ghVersion && !h.ghAuthenticated);
}

/** A launch-time gate that reports which required CLI tools are missing and links to
 * their install pages. Non-blocking: the user can continue with reduced functionality. */
export function DependencyGate({
  health,
  onRecheck,
  onClose,
}: {
  health: Health;
  onRecheck: () => Promise<void> | void;
  onClose: () => void;
}) {
  const [checking, setChecking] = useState(false);
  const list = tools(health);
  const missing = list.filter((t) => !t.version).length;
  const needsAuth = !!health.ghVersion && !health.ghAuthenticated;

  async function recheck() {
    setChecking(true);
    try {
      await onRecheck();
    } finally {
      setChecking(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center bg-black/60 p-4 pt-[8vh]"
      onClick={onClose}
    >
      <div
        className="flex max-h-[84vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-neutral-700 bg-neutral-900 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center gap-2 border-b border-neutral-800 px-5 py-3">
          <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />
          <h2 className="text-sm font-semibold text-neutral-100">
            {missing > 0 ? "Outils requis manquants" : "Configuration des outils"}
          </h2>
          <button
            onClick={onClose}
            className="ml-auto rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="overflow-auto px-5 py-4">
          <p className="mb-4 text-sm text-neutral-400">
            gitui s'appuie sur ces outils en ligne de commande. Installe ceux qui manquent,
            puis clique <strong className="text-indigo-300">Revérifier</strong>.
          </p>

          <ul className="space-y-2.5">
            {list.map((t) => {
              const ok = !!t.version;
              return (
                <li
                  key={t.name}
                  className={`rounded-lg border px-3 py-2.5 ${
                    ok
                      ? "border-neutral-800 bg-neutral-900"
                      : "border-amber-900/60 bg-amber-950/20"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {ok ? (
                      <Check className="h-4 w-4 shrink-0 text-emerald-400" />
                    ) : (
                      <X className="h-4 w-4 shrink-0 text-amber-400" />
                    )}
                    <span className="text-sm font-medium text-neutral-100">{t.name}</span>
                    {ok ? (
                      <span className="ml-auto truncate font-mono text-[11px] text-emerald-400/90">
                        {t.version}
                      </span>
                    ) : (
                      <button
                        onClick={() => safeOpen(t.url)}
                        className="ml-auto inline-flex shrink-0 items-center gap-1 rounded-md bg-indigo-600 px-2 py-1 text-[11px] font-medium text-white hover:bg-indigo-500"
                      >
                        Installer <ExternalLink className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                  <p className="mt-1 text-[11px] leading-relaxed text-neutral-500">{t.desc}</p>
                  {!ok && (
                    <code className="mt-1.5 block overflow-x-auto rounded bg-black/40 px-2 py-1 font-mono text-[11px] text-neutral-300">
                      {t.install}
                    </code>
                  )}
                </li>
              );
            })}
          </ul>

          <p className="mt-3 text-[11px] leading-relaxed text-neutral-500">
            <strong className="text-neutral-400">Claude Code</strong> (aides IA) est
            vérifié au moment où tu lances une fonction IA — pas besoin de l'installer pour
            le reste de l'app.
          </p>

          {needsAuth && (
            <div className="mt-3 rounded-lg border border-amber-900/60 bg-amber-950/20 px-3 py-2.5">
              <div className="flex items-center gap-2">
                <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />
                <span className="text-sm font-medium text-neutral-100">
                  GitHub CLI non connecté
                </span>
              </div>
              <p className="mt-1 text-[11px] leading-relaxed text-neutral-500">
                Connecte-toi pour accéder aux PRs et issues. Lance cette commande dans un
                terminal :
              </p>
              <code className="mt-1.5 block rounded bg-black/40 px-2 py-1 font-mono text-[11px] text-neutral-300">
                gh auth login
              </code>
            </div>
          )}
        </div>

        <footer className="flex items-center gap-2 border-t border-neutral-800 px-5 py-3">
          <span className="text-[11px] text-neutral-500">
            {missing === 0 && !needsAuth
              ? "Tout est prêt ✨"
              : `${missing > 0 ? `${missing} outil(s) manquant(s)` : "Connexion requise"}`}
          </span>
          <button
            onClick={recheck}
            disabled={checking}
            className="ml-auto inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1.5 text-xs font-medium text-neutral-200 hover:bg-neutral-800 disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${checking ? "animate-spin" : ""}`} />
            Revérifier
          </button>
          <button
            onClick={onClose}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
          >
            Continuer
          </button>
        </footer>
      </div>
    </div>
  );
}
