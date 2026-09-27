import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface Status {
  replay_dir: string | null;
  replay_dir_detected: boolean;
  db_path: string;
  http_url: string | null;
  matches: number;
  backfill: { done: number; total: number; finished: boolean };
  last_error: string | null;
}

interface UsageStat {
  name: string;
  picks: number;
  wins: number;
  pick_rate: number;
  win_rate: number;
}

interface Stats {
  matches: number;
  player_results: number;
  units: UsageStat[];
  specialists: UsageStat[];
}

interface Facets {
  versions: [number, number][];
  modes: [string, string, number][];
  main_player: [string, string] | null;
}

interface MatchRow {
  battle_id: string;
  version: number;
  match_mode: string;
  game_mode: string;
  rounds: number;
  occurred_on: string;
  winners: string[];
  losers: string[];
}

interface StatsFilter {
  min_version?: number;
  max_version?: number;
  match_mode?: string;
  game_mode?: string;
  player_id?: string;
}

type WatchEvent =
  | { kind: "backfill_progress"; done: number; total: number }
  | { kind: "ingested"; file: string; outcome: { status: "added" | "duplicate"; battle_id: string } }
  | { kind: "failed"; file: string; error: string };

/** Win rates from fewer games than this are dimmed: too few to mean much. */
const SMALL_SAMPLE = 10;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const pct = (x: number) => `${Math.round(x * 100)}%`;

const MATCH_MODES: Record<string, string> = {
  VS_1_1: "1v1",
  VS_2_2: "2v2",
  VS_4_Scuffle: "4-player FFA",
};
const modeLabel = (matchMode: string, gameMode: string) =>
  `${MATCH_MODES[matchMode] ?? matchMode} · ${gameMode.replace(/([a-z])([A-Z])/g, "$1 $2")}`;

let facets: Facets | null = null;

function currentFilter(): StatsFilter {
  const filter: StatsFilter = {};
  const version = $<HTMLSelectElement>("filter-version").value;
  if (version) {
    filter.min_version = filter.max_version = Number(version);
  }
  const mode = $<HTMLSelectElement>("filter-mode").value;
  if (mode) {
    const [matchMode, gameMode] = mode.split("|");
    filter.match_mode = matchMode;
    filter.game_mode = gameMode;
  }
  if ($<HTMLInputElement>("filter-mine").checked && facets?.main_player) {
    filter.player_id = facets.main_player[0];
  }
  return filter;
}

function fillSelect(select: HTMLSelectElement, options: [string, string][]) {
  const previous = select.value;
  select.replaceChildren(
    ...options.map(([value, label]) => {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      return option;
    }),
  );
  if (options.some(([value]) => value === previous)) {
    select.value = previous;
  }
}

async function refreshFacets() {
  const first = facets === null;
  facets = await invoke<Facets>("facets");
  const versionSelect = $<HTMLSelectElement>("filter-version");
  fillSelect(versionSelect, [
    ["", "All patches"],
    ...facets.versions.map(([v, n]): [string, string] => [String(v), `${v} (${n})`]),
  ]);
  // Start on the newest patch: stats from different balance patches don't mix well.
  if (first && facets.versions.length > 0) {
    versionSelect.value = String(facets.versions[0][0]);
  }
  fillSelect($<HTMLSelectElement>("filter-mode"), [
    ["", "All modes"],
    ...facets.modes.map(([m, g, n]): [string, string] => [`${m}|${g}`, `${modeLabel(m, g)} (${n})`]),
  ]);
  const mine = $<HTMLInputElement>("filter-mine");
  mine.disabled = !facets.main_player;
  $("filter-mine-label").textContent = facets.main_player
    ? `Only ${facets.main_player[1]}'s side`
    : "Only my side";
}

function el(tag: string, className?: string, text?: string) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function pickBar(rate: number) {
  const bar = el("div", "bar");
  const track = el("div", "track");
  const fill = el("div", "fill");
  fill.style.width = `${Math.min(rate, 1) * 100}%`;
  track.append(fill);
  bar.append(track, el("span", "value", pct(rate)));
  return bar;
}

function winBar(rate: number) {
  const bar = el("div", "bar diverging");
  const track = el("div", "track");
  const delta = rate - 0.5;
  const fill = el("div", `fill ${delta >= 0 ? "win" : "loss"}`);
  fill.style.width = `${Math.abs(delta) * 100}%`;
  track.append(fill, el("div", "midline"));
  bar.append(track, el("span", "value", pct(rate)));
  return bar;
}

function renderUsage(container: HTMLElement, rows: UsageStat[], results: number, noun: string) {
  const heads = [
    ["", "Name"],
    ["", "Pick rate"],
    ["", "Win rate"],
    ["num", "Games"],
  ].map(([cls, text]) => {
    const head = el("div", `head ${cls}`, text);
    head.setAttribute("role", "columnheader");
    return head;
  });
  const body = rows.map((stat) => {
    const row = el("div", stat.picks < SMALL_SAMPLE ? "row small" : "row");
    row.setAttribute("role", "row");
    row.append(
      el("div", "name", stat.name),
      pickBar(stat.pick_rate),
      winBar(stat.win_rate),
      el("div", "games", String(stat.picks)),
    );
    for (const cell of row.children) cell.setAttribute("role", "cell");
    row.dataset.tip = JSON.stringify({ ...stat, results, noun });
    return row;
  });
  container.replaceChildren(...heads, ...body);
  if (rows.length === 0) {
    container.append(el("div", "empty", "No games match these filters yet."));
  }
}

function renderMatches(matches: MatchRow[]) {
  const list = $("matches");
  list.replaceChildren(
    ...matches.map((m) => {
      const item = el("li");
      const when = new Date(m.occurred_on);
      const players = el("span");
      players.append(
        el("span", "won", m.winners.join(", ") || "—"),
        document.createTextNode(" beat "),
        el("span", "", m.losers.join(", ") || "—"),
      );
      item.append(
        el("span", "when", when.toLocaleDateString()),
        players,
        el("span", "meta", `${modeLabel(m.match_mode, m.game_mode)} · ${m.rounds} rounds · v${m.version}`),
      );
      return item;
    }),
  );
  if (matches.length === 0) {
    list.append(el("li", "empty", "No matches yet. Play a game and it will show up here."));
  }
}

async function refreshStats() {
  const stats = await invoke<Stats>("stats", { filter: currentFilter() });
  renderUsage($("units"), stats.units, stats.player_results, "unit");
  renderUsage($("specialists"), stats.specialists, stats.player_results, "specialist");
  $("summary").textContent =
    `${stats.matches} matches · ${stats.player_results} player results`;
  renderMatches(await invoke<MatchRow[]>("recent_matches", { limit: 15 }));
}

async function refreshStatus(): Promise<Status> {
  const status = await invoke<Status>("status");
  $("replay-dir").textContent = status.replay_dir ?? "not found";
  $("replay-dir").title = status.replay_dir ?? "";
  $("replay-dir-badge").hidden = !status.replay_dir_detected;
  $("auto-dir").hidden = status.replay_dir_detected || !status.replay_dir;

  const { done, total, finished } = status.backfill;
  const parts = [`${status.matches} matches stored`];
  if (status.replay_dir && !finished && total > 0) {
    parts.unshift(`Reading replays ${done}/${total}`);
  } else if (status.replay_dir) {
    parts.push("watching for new games");
  }
  const statusEl = $("status");
  statusEl.textContent = parts.join(" · ");
  statusEl.title = [status.http_url && `API: ${status.http_url}`, `Database: ${status.db_path}`]
    .filter(Boolean)
    .join("\n");

  const error = $("error");
  error.hidden = !status.last_error;
  error.textContent = status.last_error ?? "";
  return status;
}

let refreshTimer: number | undefined;
/** Coalesce bursts of new replays (e.g. the initial backfill) into one redraw. */
function scheduleRefresh(delay = 800) {
  window.clearTimeout(refreshTimer);
  refreshTimer = window.setTimeout(async () => {
    await refreshFacets();
    await refreshStats();
  }, delay);
}

function setupTooltip() {
  const tip = $("tooltip");
  document.addEventListener("mousemove", (event) => {
    const row = (event.target as HTMLElement).closest<HTMLElement>(".row");
    if (!row?.dataset.tip) {
      tip.hidden = true;
      return;
    }
    const s = JSON.parse(row.dataset.tip) as UsageStat & { results: number; noun: string };
    tip.replaceChildren(
      el("strong", "", s.name),
      el("div", "", `Fielded in ${s.picks} of ${s.results} results (${pct(s.pick_rate)})`),
      el("div", "", `Won ${s.wins} of ${s.picks} (${pct(s.win_rate)})`),
    );
    if (s.picks < SMALL_SAMPLE) {
      tip.append(el("div", "muted", `Fewer than ${SMALL_SAMPLE} games: treat the win rate as noise.`));
    }
    tip.hidden = false;
    const x = Math.min(event.clientX + 14, window.innerWidth - tip.offsetWidth - 8);
    const y = Math.min(event.clientY + 14, window.innerHeight - tip.offsetHeight - 8);
    tip.style.left = `${x}px`;
    tip.style.top = `${y}px`;
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  setupTooltip();

  $("choose-dir").addEventListener("click", async () => {
    const dir = await open({ directory: true, multiple: false, title: "Mechabellum replay folder" });
    if (typeof dir === "string") {
      try {
        await invoke("set_replay_dir", { dir });
      } catch (e) {
        $("error").hidden = false;
        $("error").textContent = String(e);
      }
      await refreshStatus();
    }
  });
  $("auto-dir").addEventListener("click", async () => {
    await invoke("set_replay_dir", { dir: null });
    await refreshStatus();
  });
  for (const id of ["filter-version", "filter-mode", "filter-mine"]) {
    $(id).addEventListener("change", () => refreshStats());
  }

  await listen<WatchEvent>("replay-event", (event) => {
    const e = event.payload;
    if (e.kind === "ingested" && e.outcome.status === "added") {
      scheduleRefresh();
    } else if (e.kind === "backfill_progress" && e.done === e.total) {
      scheduleRefresh(0);
    }
    refreshStatus();
  });

  await refreshStatus();
  await refreshFacets();
  await refreshStats();
  // Keep the status line fresh while a large backfill runs.
  window.setInterval(refreshStatus, 3000);
});
