import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";
import { DownloadCloud, X } from "lucide-react";
import { api } from "../lib/api";

// Where package-manager users (deb / rpm / pacman) go to grab the new build.
const RELEASES_URL = "https://github.com/Corentin-vidonne/gitui/releases/latest";

// Top banner shown once a newer release exists. On installs that can self-update (Windows,
// macOS, Linux AppImage → channel "updater") it offers a one-click download+install+relaunch;
// on package-manager installs (channel "manager") it just links to the release so the user
// updates via apt/dnf/pacman. The version check (`check()`) runs on every platform — its fetch
// happens in Rust, so the webview CSP doesn't need to allow GitHub.
export function AppUpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [channel, setChannel] = useState<"updater" | "manager">("updater");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [ch, found] = await Promise.all([
          api.updateChannel().catch(() => "updater" as const),
          check(),
        ]);
        if (alive && found) {
          setChannel(ch);
          setUpdate(found);
        }
      } catch {
        // Offline, or no manifest published yet — stay silent.
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  if (!update || dismissed) return null;

  async function install() {
    if (!update) return;
    setBusy(true);
    setError(null);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-x-0 top-0 z-[60] flex items-center justify-center gap-3 border-b border-indigo-500/30 bg-indigo-950/95 px-4 py-2 text-sm text-indigo-100 shadow-lg backdrop-blur">
      <DownloadCloud className="h-4 w-4 shrink-0 text-indigo-300" />
      <span>
        gitui <b>{update.version}</b> est disponible
        {update.currentVersion ? ` — tu as ${update.currentVersion}` : ""}.
        {channel === "manager" && (
          <span className="text-indigo-300/80">
            {" "}
            Mets à jour via ton gestionnaire de paquets.
          </span>
        )}
        {error && <span className="ml-2 text-red-300">{error}</span>}
      </span>
      {channel === "updater" ? (
        <button
          onClick={install}
          disabled={busy}
          className="shrink-0 rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
        >
          {busy ? "Mise à jour…" : "Mettre à jour"}
        </button>
      ) : (
        <button
          onClick={() => void openUrl(RELEASES_URL)}
          className="shrink-0 rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-500"
        >
          Télécharger
        </button>
      )}
      <button
        onClick={() => setDismissed(true)}
        title="Ignorer"
        className="shrink-0 rounded p-1 text-indigo-300 hover:bg-indigo-900/60"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
