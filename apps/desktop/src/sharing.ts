import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { $, el } from "./util";

interface SyncSettings {
  enabled: boolean;
  server: string;
  key: string;
}

interface SyncStatus {
  running: boolean;
  shared: number;
  total: number;
  rejected: number;
  last_error: string | null;
}

interface SyncRejection {
  battle_id: string;
  reason: string;
  occurred_on: string;
  version: number;
}

let saved: SyncSettings = { enabled: false, server: "", key: "" };
let lastRejected = -1;

const ready = (s: SyncSettings) => s.enabled && s.server.trim() !== "" && s.key.trim() !== "";

function formValues(): SyncSettings {
  return {
    enabled: $<HTMLInputElement>("sync-enabled").checked,
    server: $<HTMLInputElement>("sync-server").value.trim(),
    key: $<HTMLInputElement>("sync-key").value.trim(),
  };
}

function fillForm(s: SyncSettings) {
  $<HTMLInputElement>("sync-enabled").checked = s.enabled;
  $<HTMLInputElement>("sync-server").value = s.server;
  $<HTMLInputElement>("sync-key").value = s.key;
  $<HTMLButtonElement>("sync-now").disabled = !ready(s);
}

function renderStatus(status: SyncStatus) {
  const { shared, total, rejected, running } = status;
  const done = shared + rejected;
  const fill = total > 0 ? Math.min(1, done / total) : 0;
  $("sync-bar-fill").style.width = `${(fill * 100).toFixed(1)}%`;
  $("sync-progress").classList.toggle("running", running);

  let line: string;
  if (!ready(saved)) {
    line = saved.enabled
      ? "Add a server address and upload key to start sharing."
      : "Sharing is off. Nothing leaves this computer.";
  } else if (total === 0 && running) {
    line = "Checking which matches the server already has…";
  } else if (total === 0) {
    line = "Waiting for the first check with the server.";
  } else {
    line = `Shared ${shared.toLocaleString()} of ${total.toLocaleString()} matches`;
    if (rejected > 0) line += `, ${rejected.toLocaleString()} not accepted`;
    line += running ? ". Sending…" : ".";
  }
  $("sync-line").textContent = line;

  const error = $("sync-error");
  error.hidden = !status.last_error || !ready(saved);
  error.textContent = status.last_error
    ? `Couldn't finish sharing: ${status.last_error}. It will try again in a few minutes.`
    : "";

  if (rejected !== lastRejected) {
    lastRejected = rejected;
    refreshRejections();
  }
}

async function refreshRejections() {
  const rows = await invoke<SyncRejection[]>("sync_rejections");
  const box = $<HTMLDetailsElement>("sync-rejected");
  box.hidden = rows.length === 0;
  $("sync-rejected-summary").textContent =
    `${rows.length} ${rows.length === 1 ? "match wasn't" : "matches weren't"} accepted`;
  $("sync-rejected-list").replaceChildren(
    ...rows.map((r) => {
      const item = el("li");
      item.append(
        el("span", "rejected-when", `${r.occurred_on.slice(0, 10)} · patch ${r.version}`),
        el("span", "rejected-reason", r.reason),
      );
      item.title = r.battle_id;
      return item;
    }),
  );
}

async function refreshStatus() {
  renderStatus(await invoke<SyncStatus>("sync_status"));
}

export async function setupSharing() {
  saved = await invoke<SyncSettings>("sync_settings");
  fillForm(saved);

  const form = $<HTMLFormElement>("sharing-form");
  const note = $("sync-saved");
  form.addEventListener("input", () => {
    note.textContent = "";
  });
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const next = formValues();
    if (next.enabled && next.server && !/^https?:\/\//i.test(next.server)) {
      note.textContent = "The server address needs to start with https:// or http://";
      return;
    }
    try {
      await invoke("set_sync_settings", { sync: next });
      saved = next;
      fillForm(saved);
      note.textContent = ready(saved) ? "Saved. Sharing starts now." : "Saved.";
      lastRejected = -1;
      await refreshStatus();
    } catch (e) {
      note.textContent = `Couldn't save: ${e}`;
    }
  });
  $("sync-now").addEventListener("click", async () => {
    await invoke("sync_now");
    note.textContent = "";
  });

  await listen<SyncStatus>("sync-status", (event) => renderStatus(event.payload));
  await refreshStatus();
}
