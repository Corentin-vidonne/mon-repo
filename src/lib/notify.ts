import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let ensured: Promise<boolean> | null = null;

/** Ask for desktop-notification permission once, then cache the answer. */
async function ensurePermission(): Promise<boolean> {
  if (!ensured) {
    ensured = (async () => {
      try {
        let granted = await isPermissionGranted();
        if (!granted) granted = (await requestPermission()) === "granted";
        return granted;
      } catch {
        return false;
      }
    })();
  }
  return ensured;
}

/** Send a desktop notification (no-op if permission is denied/unavailable). */
export async function notify(title: string, body: string): Promise<void> {
  try {
    if (await ensurePermission()) sendNotification({ title, body });
  } catch {
    /* notifications are best-effort */
  }
}
