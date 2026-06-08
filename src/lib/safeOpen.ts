import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Open an external URL only if it uses the http(s) scheme.
 *
 * Many URLs we open come from `gh` / `git` output — PR URLs, issue URLs, and especially
 * CI check `link`s — which are attacker-influenceable (anyone who can configure a check on
 * a repo the user reviews controls them). Handing a `file:` or custom-scheme value to the
 * OS opener can launch local files or apps with a single click, so we validate first.
 * Non-URLs and disallowed schemes are silently ignored.
 */
export function safeOpen(raw: string): void {
  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    return;
  }
  if (u.protocol === "http:" || u.protocol === "https:") {
    void openUrl(u.toString());
  }
}
