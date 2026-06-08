import { X, Sparkles } from "lucide-react";

type Item = { id?: string; name: string; desc: string };

function Section({
  title,
  items,
  onTest,
}: {
  title: string;
  items: Item[];
  onTest: (id: string) => void;
}) {
  return (
    <div className="mb-4">
      <h3 className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-indigo-300/80">
        {title}
      </h3>
      <ul className="space-y-1.5">
        {items.map((it) => (
          <li key={it.name} className="flex items-start gap-2 text-sm">
            <div className="min-w-0 flex-1">
              <span className="font-medium text-neutral-200">{it.name}</span>
              <span className="text-neutral-400"> — {it.desc}</span>
            </div>
            {it.id && (
              <button
                onClick={() => onTest(it.id as string)}
                title="Lancer un guide pour essayer cette fonction"
                className="shrink-0 rounded border border-indigo-700 px-2 py-0.5 text-[11px] font-medium text-indigo-300 hover:bg-indigo-950/40"
              >
                Tester
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Reference page describing everything the app does. Each feature has a "Tester" button
 * that launches a focused, live guide for it; the header launches the full tour. */
export function HelpPage({
  onClose,
  onStartTour,
  onTest,
}: {
  onClose: () => void;
  onStartTour: () => void;
  onTest: (id: string) => void;
}) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-4 pt-[6vh]"
      onClick={onClose}
    >
      <div
        className="flex max-h-[86vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-neutral-700 bg-neutral-900 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center gap-2 border-b border-neutral-800 px-5 py-3">
          <h2 className="text-sm font-semibold text-neutral-100">Tout ce que fait gitui</h2>
          <button
            onClick={onStartTour}
            className="ml-auto inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
          >
            <Sparkles className="h-3.5 w-3.5" /> Revoir le guide
          </button>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="overflow-auto px-5 py-4">
          <p className="mb-4 text-sm text-neutral-400">
            gitui est un outil de <strong className="text-neutral-200">piles de branches</strong>{" "}
            (stacked PRs). Clique <strong className="text-indigo-300">Tester</strong> à côté d'une
            fonction pour la voir en action dans l'app, ou{" "}
            <strong className="text-indigo-300">Revoir le guide</strong> pour le tour général.
          </p>

          <Section
            title="Vues"
            onTest={onTest}
            items={[
              { id: "view-graph", name: "Branch graph", desc: "l'arbre de la pile ; glisse une branche sur une autre pour la re-parenter" },
              { id: "view-commits", name: "Commit graph", desc: "le DAG des commits, avec recherche et filtre" },
              { id: "tree", name: "Tree", desc: "la pile en liste, avec les actions par branche" },
              { id: "prs", name: "Pull requests", desc: "liste et détail, relecture, checks CI" },
              { id: "issues", name: "Issues", desc: "liste et détail des issues" },
              { id: "docs", name: "Docs", desc: "les fichiers Markdown du dépôt" },
            ]}
          />
          <Section
            title="Branches"
            onTest={onTest}
            items={[
              { id: "new-branch", name: "Nouvelle branche", desc: "au sommet de la pile (nom suggéré par l'IA ✨)" },
              { id: "branch-ops", name: "Checkout / Set parent / Track / Untrack", desc: "gérer la place dans la pile" },
              { id: "branch-ops", name: "Restack", desc: "rebaser une branche sur son parent" },
              { id: "branch-ops", name: "Merge", desc: "fusionner deux branches via l'assistant" },
            ]}
          />
          <Section
            title="Commits"
            onTest={onTest}
            items={[
              { id: "commit-ops", name: "Reword", desc: "ré-écrire le message (génération IA ✨)" },
              { id: "commit-ops", name: "Split", desc: "découper un commit en deux, par fichier" },
              { id: "commit-ops", name: "Drop / Squash / Move", desc: "supprimer, fusionner, réordonner" },
              { id: "commit-ops", name: "Cherry-pick", desc: "appliquer un commit sur une autre branche" },
              { id: "commit-ops", name: "AI Review", desc: "relecture structurée d'un commit" },
            ]}
          />
          <Section
            title="La pile"
            onTest={onTest}
            items={[
              { id: "sync", name: "Sync", desc: "fetch + fast-forward + nettoyage PR + restack" },
              { id: "sync", name: "Restack all", desc: "rebaser toute la pile" },
              { id: "submit", name: "Submit", desc: "pousser et ouvrir/mettre à jour les PRs (description IA ✨)" },
              { id: "sync", name: "Undo", desc: "annuler la dernière opération qui réécrit l'historique" },
            ]}
          />
          <Section
            title="Stashes"
            onTest={onTest}
            items={[
              { id: "stash", name: "Voir le contenu", desc: "la liste des fichiers de chaque stash" },
              { id: "stash", name: "Apply / Pop / Drop", desc: "appliquer, appliquer+supprimer, supprimer" },
              { id: "stash", name: "Stasher", desc: "mettre de côté les changements en cours" },
            ]}
          />
          <Section
            title="Assistant IA (Claude)"
            onTest={onTest}
            items={[
              { id: "commit-ops", name: "Summary / Detailed", desc: "analyse d'un commit ou d'une PR" },
              { id: "claude", name: "Demander à Claude", desc: "chat libre sur tout le dépôt" },
              { id: "branch-ops", name: "Aide au merge", desc: "guide le merge avec approbation des commandes" },
              { name: "Résoudre les conflits", desc: "proposition IA par fichier, ou tout d'un coup (au restack)" },
              { name: "Digest", desc: "résumé « depuis ta dernière visite » à l'ouverture" },
            ]}
          />
          <Section
            title="Pull requests"
            onTest={onTest}
            items={[
              { id: "prs", name: "Review", desc: "Approuver / Demander des changements / Commenter" },
              { id: "prs", name: "Checks CI", desc: "voir chaque check, ouvrir ses logs ; notif de fin" },
              { id: "view-graph", name: "Pastilles", desc: "état PR, CI, review, avance/retard — d'un coup d'œil" },
            ]}
          />
          <Section
            title="Productivité"
            onTest={onTest}
            items={[
              { id: "palette", name: "Palette de commandes", desc: "Ctrl/⌘ + K : aller quelque part ou lancer une action" },
              { id: "shortcuts", name: "Raccourcis clavier", desc: "? pour l'aide ; 1–6 vues ; s/r/n/p/u actions" },
              { id: "commit-search", name: "Recherche de commits", desc: "filtrer le graphe par message / sha / auteur" },
            ]}
          />
          <Section
            title="Réglages"
            onTest={onTest}
            items={[
              { name: "Thème", desc: "Classique / Modern" },
              { name: "Interface assistant", desc: "Chat (bulles) ou Terminal ; réponse progressive" },
              { name: "Backend IA", desc: "Claude (cloud Anthropic) ou Ollama (modèles locaux gratuits)" },
              { name: "Notifications", desc: "activité PRs/issues/CI + intervalle de sondage" },
            ]}
          />
        </div>
      </div>
    </div>
  );
}
