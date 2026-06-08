import { useState } from "react";
import { safeOpen } from "../lib/safeOpen";
import {
  Layers,
  Sparkles,
  Cloud,
  Server,
  ExternalLink,
  RefreshCw,
  Search,
} from "lucide-react";

const CLAUDE_DOCS = "https://docs.claude.com/en/docs/claude-code/setup";
const OLLAMA_DOWNLOAD = "https://ollama.com/download";

const FEATURES: { Icon: typeof Layers; title: string; desc: string }[] = [
  {
    Icon: Layers,
    title: "Visualise & réorganise ta pile",
    desc: "Arbre des branches, glisser-déposer pour re-parenter, graphe des commits.",
  },
  {
    Icon: RefreshCw,
    title: "Restack · Sync · Submit",
    desc: "Rebase en cascade, fast-forward, push + ouverture/maj des PRs GitHub — et Undo.",
  },
  {
    Icon: Sparkles,
    title: "Aides IA",
    desc: "Messages de commit, descriptions de PR, revues, résolution de conflits, merges guidés, chat.",
  },
  {
    Icon: Search,
    title: "Au quotidien",
    desc: "Review de PR, checks CI, stashes, recherche de commits, palette Ctrl/⌘ + K.",
  },
];

/** First-launch welcome: a short multi-step intro to what gitui does, ending on the AI
 * engine choice (install Claude Code or Ollama). Calls `onFinish(true)` to chain into the
 * interactive tour, or `onFinish(false)` when skipped. */
export function WelcomeScreen({ onFinish }: { onFinish: (startTour: boolean) => void }) {
  const [step, setStep] = useState(0);
  const TOTAL = 3;
  const last = step === TOTAL - 1;

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/70 p-4">
      <div className="flex w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-neutral-700 bg-neutral-900 shadow-2xl">
        <div className="min-h-[296px] px-7 pb-3 pt-8">
          {step === 0 && (
            <div className="text-center">
              <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-indigo-600/20 ring-1 ring-indigo-500/40">
                <Layers className="h-7 w-7 text-indigo-300" />
              </div>
              <h2 className="text-lg font-semibold text-neutral-100">Bienvenue dans gitui</h2>
              <p className="mx-auto mt-2 max-w-sm text-sm leading-relaxed text-neutral-400">
                Ton outil de <strong className="text-neutral-200">piles de branches</strong>{" "}
                (stacked PRs), local et gratuit — façon Graphite. Visualise l'arbre de tes
                branches, <strong className="text-neutral-200">restack</strong> en cascade, et
                ouvre/maj tes PRs GitHub.
              </p>
            </div>
          )}

          {step === 1 && (
            <div>
              <h2 className="mb-3 text-lg font-semibold text-neutral-100">
                Ce que tu peux faire
              </h2>
              <ul className="space-y-3">
                {FEATURES.map((f) => (
                  <li key={f.title} className="flex gap-3">
                    <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-neutral-800 ring-1 ring-neutral-700">
                      <f.Icon className="h-4 w-4 text-indigo-300" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-neutral-200">{f.title}</div>
                      <div className="text-[11px] leading-relaxed text-neutral-500">{f.desc}</div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {step === 2 && (
            <div>
              <div className="flex items-center gap-2">
                <Sparkles className="h-5 w-5 text-indigo-300" />
                <h2 className="text-lg font-semibold text-neutral-100">Active les fonctions IA</h2>
              </div>
              <p className="mt-2 text-sm leading-relaxed text-neutral-400">
                Les aides IA passent par la CLI{" "}
                <strong className="text-neutral-200">Claude Code</strong>. Pour la meilleure
                expérience, installe l'une des deux — au choix :
              </p>

              <div className="mt-3 space-y-2">
                <div className="flex items-center gap-3 rounded-lg border border-neutral-700 bg-neutral-800/40 p-3">
                  <Cloud className="h-5 w-5 shrink-0 text-indigo-300" />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-neutral-100">Claude Code (cloud)</div>
                    <div className="text-[11px] text-neutral-500">
                      Avec ton compte Anthropic — modèles Claude.
                    </div>
                  </div>
                  <button
                    onClick={() => safeOpen(CLAUDE_DOCS)}
                    className="inline-flex shrink-0 items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1.5 text-[11px] font-medium text-white hover:bg-indigo-500"
                  >
                    Installer <ExternalLink className="h-3 w-3" />
                  </button>
                </div>

                <div className="flex items-center gap-3 rounded-lg border border-neutral-700 bg-neutral-800/40 p-3">
                  <Server className="h-5 w-5 shrink-0 text-emerald-300" />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-neutral-100">Ollama (local & gratuit)</div>
                    <div className="text-[11px] text-neutral-500">
                      Modèles locaux ou cloud Ollama, sans compte Anthropic.
                    </div>
                  </div>
                  <button
                    onClick={() => safeOpen(OLLAMA_DOWNLOAD)}
                    className="inline-flex shrink-0 items-center gap-1 rounded-md bg-emerald-600 px-2.5 py-1.5 text-[11px] font-medium text-white hover:bg-emerald-500"
                  >
                    Installer <ExternalLink className="h-3 w-3" />
                  </button>
                </div>
              </div>

              <p className="mt-2.5 text-[11px] leading-relaxed text-neutral-500">
                Tu pourras choisir le moteur et le modèle dans{" "}
                <strong className="text-neutral-400">Réglages → IA</strong> à tout moment.
              </p>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between border-t border-neutral-800 px-7 py-3">
          <button
            onClick={() => onFinish(false)}
            className="text-xs text-neutral-500 hover:text-neutral-300"
          >
            Passer
          </button>
          <div className="flex items-center gap-1.5">
            {Array.from({ length: TOTAL }).map((_, i) => (
              <span
                key={i}
                className={`h-1.5 rounded-full transition-all ${
                  i === step ? "w-4 bg-indigo-400" : "w-1.5 bg-neutral-700"
                }`}
              />
            ))}
          </div>
          <div className="flex gap-2">
            {step > 0 && (
              <button
                onClick={() => setStep(step - 1)}
                className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-300 hover:bg-neutral-800"
              >
                Précédent
              </button>
            )}
            {last ? (
              <button
                onClick={() => onFinish(true)}
                className="rounded-md bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
              >
                Démarrer le guide
              </button>
            ) : (
              <button
                onClick={() => setStep(step + 1)}
                className="rounded-md bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
              >
                Suivant
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
