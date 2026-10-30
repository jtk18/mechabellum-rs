import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { $, el } from "./util";

/** A release newer than this copy, from the update endpoint's latest.json. */
interface Available {
  version: string;
  notes: string | null;
  published: number | null;
}

/** Mirrors `updates::UpdateStatus` in src-tauri. Times are seconds since 1970. */
interface UpdateStatus {
  current: string;
  supported: boolean;
  auto_check: boolean;
  checking: boolean;
  installing: boolean;
  progress: number | null;
  available: Available | null;
  checked_at: number | null;
  error: string | null;
  updated_from: string | null;
}

let status: UpdateStatus | null = null;
/** The version put off with "Later": its banner stays hidden until a newer one or "Check now". */
let putOff: string | null = null;
/** The note saying an update just installed was dismissed. */
let updatedSeen = false;
/** Why the last install from the banner failed. */
let installError: string | null = null;

function when(secs: number) {
  const date = new Date(secs * 1000);
  return date.toDateString() === new Date().toDateString()
    ? date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}

function installLabel(s: UpdateStatus) {
  if (!s.installing) return "Install and restart";
  if (s.progress !== null && s.progress < 1) return `Downloading ${Math.floor(s.progress * 100)}%`;
  return "Installing…";
}

function renderBanner(s: UpdateStatus) {
  const offer = s.available && (s.installing || s.available.version !== putOff) ? s.available : null;
  const updatedFrom = !offer && !updatedSeen ? s.updated_from : null;
  const title = $("update-title");
  const notes = $("update-notes");
  const install = $<HTMLButtonElement>("update-install");
  const later = $<HTMLButtonElement>("update-later");
  $("update-banner").hidden = !offer && !updatedFrom;

  if (offer) {
    title.replaceChildren(`Version ${offer.version} is available `, el("small", "", `you have ${s.current}`));
    // Plain text: latest.json isn't signed, only the download is.
    notes.textContent = offer.notes?.trim() ?? "";
    notes.hidden = !offer.notes;
    install.hidden = false;
    install.disabled = s.installing;
    install.textContent = installLabel(s);
    later.hidden = s.installing;
    later.textContent = "Later";
  } else if (updatedFrom) {
    title.replaceChildren(`Updated to version ${s.current} `, el("small", "", `from ${updatedFrom}`));
    notes.hidden = true;
    install.hidden = true;
    later.hidden = false;
    later.textContent = "OK";
  }

  const error = $("update-error");
  error.hidden = !installError || !offer || s.installing;
  error.textContent = installError ?? "";
}

function settingsLine(s: UpdateStatus) {
  if (!s.supported) return "This copy runs from a build folder, so it doesn't update itself. Installed copies do.";
  if (s.installing) return `Installing version ${s.available?.version ?? ""}…`;
  if (s.checking) return "Checking for updates…";
  const checked = s.checked_at ? ` Checked ${when(s.checked_at)}.` : "";
  // Errors arrive as whole sentences.
  if (s.error) return s.error + checked;
  if (s.available) return `Version ${s.available.version} is available.${checked}`;
  if (s.checked_at) return `Up to date.${checked}`;
  if (s.updated_from) return `Updated from ${s.updated_from}.`;
  return s.auto_check ? "Checks shortly after the app starts, then every few hours." : "Automatic checks are off.";
}

function renderSettings(s: UpdateStatus) {
  $("update-version").textContent = `version ${s.current}`;
  const auto = $<HTMLInputElement>("update-auto");
  auto.checked = s.auto_check;
  auto.disabled = !s.supported;
  const check = $<HTMLButtonElement>("update-check");
  check.disabled = !s.supported || s.checking || s.installing;
  check.textContent = s.checking ? "Checking…" : "Check now";
  $("update-line").textContent = settingsLine(s);
}

function render(s: UpdateStatus) {
  status = s;
  renderBanner(s);
  renderSettings(s);
}

/** The update banner and the Updates panel in Settings. Checks run in the app, on start and
 * every few hours; this draws what they find and starts installs. */
export async function setupUpdates() {
  $("update-install").addEventListener("click", async () => {
    installError = null;
    try {
      // On success the app restarts, so this may never return.
      await invoke("install_update");
    } catch (e) {
      installError = String(e);
      if (status) render(status);
    }
  });
  $("update-later").addEventListener("click", () => {
    if (status?.available) putOff = status.available.version;
    updatedSeen = true;
    if (status) render(status);
  });
  $("update-check").addEventListener("click", async () => {
    installError = null;
    putOff = null;
    try {
      render(await invoke<UpdateStatus>("check_for_update"));
    } catch (e) {
      $("update-line").textContent = `Couldn't check for updates: ${e}`;
    }
  });
  const auto = $<HTMLInputElement>("update-auto");
  auto.addEventListener("change", async () => {
    try {
      await invoke("set_check_for_updates", { enabled: auto.checked });
      if (status) render({ ...status, auto_check: auto.checked });
    } catch (e) {
      auto.checked = !auto.checked;
      $("update-line").textContent = `Couldn't save the setting: ${e}`;
    }
  });

  await listen<UpdateStatus>("update-status", (event) => render(event.payload));
  render(await invoke<UpdateStatus>("update_status"));
}
