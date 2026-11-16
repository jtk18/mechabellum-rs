// Setting up a match's round in the game's Testing Grounds: the panel that explains what will
// happen, asks macOS for its permissions, starts and stops a run, and shows its progress and
// result. The run itself is the grounds crate, driven from src-tauri/src/grounds.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { $, el } from "./util";

interface Info {
  supported: boolean;
  accessibility: boolean | null;
  screen_recording: boolean | null;
  running: boolean;
}

interface Finished {
  outcome: "done" | "stopped" | "failed";
  message: string;
  left_out: string[];
  differences: string[] | null;
  notes: string[];
}

let target: { battleId: string; roundIndex: number } | null = null;
let wired = false;
/** The newest log lines; older ones scroll away. */
const LOG_LINES = 200;

/** Whether this platform can drive the game (not Linux). */
export async function groundsSupported(): Promise<boolean> {
  try {
    return (await invoke<Info>("grounds_info")).supported;
  } catch {
    return false;
  }
}

export function groundsPanelOpen(): boolean {
  return !$("grounds-panel").hidden;
}

/** Open the panel for one round of a stored match. `what` names it, e.g. "Round 3 of ...". */
export async function openGrounds(battleId: string, roundIndex: number, what: string) {
  await wire();
  const info = await invoke<Info>("grounds_info");
  // A run in progress keeps its own round; don't swap the target under it.
  if (!info.running) {
    target = { battleId, roundIndex };
    $("grounds-what").textContent = what;
    $("grounds-result").hidden = true;
  }
  show(info);
  $("grounds-panel").hidden = false;
  $<HTMLButtonElement>(info.running ? "grounds-stop" : "grounds-start").focus();
}

function close() {
  $("grounds-panel").hidden = true;
}

/** Permissions (macOS) and which buttons apply. */
function show(info: Info) {
  const mac = info.accessibility !== null;
  $("grounds-access").hidden = !mac || (info.accessibility === true && info.screen_recording === true);
  const mark = (allowed: boolean | null, name: string, what: string) =>
    `${allowed ? "✓" : "✗"} ${name}: ${allowed ? "allowed" : `not allowed yet (${what})`}`;
  $("grounds-accessibility").textContent = mark(info.accessibility, "Accessibility", "to click and press keys");
  $("grounds-screen").textContent = mark(info.screen_recording, "Screen Recording", "to see the game");
  const ready = info.supported && (!mac || (info.accessibility === true && info.screen_recording === true));
  const start = $<HTMLButtonElement>("grounds-start");
  start.hidden = info.running;
  start.disabled = !ready;
  start.title = ready ? "" : "macOS hasn't allowed it yet";
  $("grounds-stop").hidden = !info.running;
  $<HTMLInputElement>("grounds-fight").disabled = info.running;
  $("grounds-run").hidden = !info.running && $("grounds-log").textContent === "";
}

function log(line: string) {
  const pre = $("grounds-log");
  const lines = (pre.textContent ? pre.textContent.split("\n") : []).concat(line);
  pre.textContent = lines.slice(-LOG_LINES).join("\n");
  pre.scrollTop = pre.scrollHeight;
}

function progress(done: number, total: number) {
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;
  $("grounds-fill").style.width = `${pct}%`;
  $("grounds-bar").setAttribute("aria-valuenow", String(pct));
  // Done, not placed: a unit the game refused counts too, and the log says which.
  $("grounds-status").textContent = total > 0 ? `${done} of ${total} units done` : "";
}

function list(title: string, items: string[]) {
  const section = el("div");
  section.append(el("h3", undefined, title));
  const ul = el("ul");
  for (const item of items) ul.append(el("li", undefined, item));
  section.append(ul);
  return section;
}

function result(f: Finished) {
  const box = $("grounds-result");
  box.replaceChildren(el("p", f.outcome === "failed" ? "error" : "grounds-message", f.message));
  if (f.left_out.length) box.append(list("Left out (Testing Grounds can't reproduce them)", f.left_out));
  if (f.differences?.length) box.append(list("Where the saved replay differs", f.differences));
  if (f.notes.length) box.append(list("Notes", f.notes));
  box.hidden = false;
}

async function refresh() {
  show(await invoke<Info>("grounds_info"));
}

async function wire() {
  if (wired) return;
  wired = true;
  await listen<string>("grounds-line", (e) => log(e.payload));
  await listen<{ done: number; total: number }>("grounds-units", (e) => progress(e.payload.done, e.payload.total));
  await listen<Finished>("grounds-finished", async (e) => {
    result(e.payload);
    await refresh();
  });
  $("grounds-start").addEventListener("click", async () => {
    if (!target) return;
    $("grounds-log").textContent = "";
    $("grounds-result").hidden = true;
    progress(0, 0);
    $("grounds-run").hidden = false;
    try {
      await invoke("grounds_start", {
        battleId: target.battleId,
        roundIndex: target.roundIndex,
        fight: $<HTMLInputElement>("grounds-fight").checked,
      });
      log("Starting. The game comes to the front; move the mouse to stop.");
    } catch (err) {
      result({ outcome: "failed", message: String(err), left_out: [], differences: null, notes: [] });
    }
    await refresh();
  });
  $("grounds-stop").addEventListener("click", () => invoke("grounds_stop"));
  $("grounds-ask").addEventListener("click", async () => show(await invoke<Info>("grounds_request_access")));
  $("grounds-restart").addEventListener("click", () => invoke("grounds_restart"));
  $("grounds-close").addEventListener("click", close);
  // Coming back from System Settings: show what was switched on.
  window.addEventListener("focus", () => {
    if (groundsPanelOpen()) refresh();
  });
  // Capturing on the window runs before the match view's own keys (Escape leaves the match).
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.key === "Escape" && groundsPanelOpen()) {
        e.stopImmediatePropagation();
        e.preventDefault();
        close();
      }
    },
    { capture: true },
  );
}
