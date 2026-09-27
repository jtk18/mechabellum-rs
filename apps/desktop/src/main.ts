import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openMatch } from "./match";
import { $, el, modeLabel, pct, type Tip } from "./util";

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

interface TrendCell {
  picks: number;
  wins: number;
}

interface VersionTrends {
  versions: { version: number; matches: number; player_results: number }[];
  units: { name: string; picks: number; cells: (TrendCell | null)[] }[];
  specialists: { name: string; picks: number; cells: (TrendCell | null)[] }[];
}

interface TechStat {
  tech: string;
  picks: number;
  wins: number;
  pick_rate: number;
  win_rate: number;
  win_rate_without: number | null;
}

interface UnitTechStats {
  unit: string;
  picks: number;
  wins: number;
  techs: TechStat[];
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

/** Units whose tech breakdown is open. */
const expandedUnits = new Set<string>();
let lastTechs = new Map<string, UnitTechStats>();

let facets: Facets | null = null;
/** Stored match count the panels were last drawn from. */
let renderedMatches = -1;

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

function techRows(unit: UnitTechStats) {
  const block = el("div", "tech-rows");
  block.setAttribute("role", "rowgroup");
  if (unit.techs.length === 0) {
    block.append(el("div", "tech-empty", "No researched techs recorded for this unit."));
    return block;
  }
  for (const t of unit.techs) {
    const row = el("div", t.picks < SMALL_SAMPLE ? "row tech small" : "row tech");
    row.setAttribute("role", "row");
    const without = t.win_rate_without === null ? "every game had it" : `${pct(t.win_rate_without)} without`;
    const winCell = el("div", "tech-win");
    winCell.append(winBar(t.win_rate), el("span", "without", without));
    row.append(el("div", "name", t.tech), pickBar(t.pick_rate), winCell, el("div", "games", String(t.picks)));
    for (const cell of row.children) cell.setAttribute("role", "cell");
    const tip: Tip = {
      title: `${unit.unit}: ${t.tech}`,
      lines: [
        `Researched in ${t.picks} of ${unit.picks} games fielding ${unit.unit} (${pct(t.pick_rate)})`,
        `Won ${t.wins} of ${t.picks} with it (${pct(t.win_rate)})`,
        t.win_rate_without === null
          ? "Every game fielding it had this tech."
          : `Won ${unit.wins - t.wins} of ${unit.picks - t.picks} without it (${pct(t.win_rate_without)})`,
      ],
      note: t.picks < SMALL_SAMPLE ? `Fewer than ${SMALL_SAMPLE} games: treat the win rate as noise.` : undefined,
    };
    row.dataset.tip = JSON.stringify(tip);
    block.append(row);
  }
  return block;
}

function renderUsage(
  container: HTMLElement,
  rows: UsageStat[],
  results: number,
  techs?: Map<string, UnitTechStats>,
) {
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
    const unitTechs = techs?.get(stat.name);
    const name = el("div", "name");
    if (unitTechs) {
      const open = expandedUnits.has(stat.name);
      const toggle = el("button", "disclosure", stat.name) as HTMLButtonElement;
      toggle.type = "button";
      toggle.setAttribute("aria-expanded", String(open));
      toggle.title = open ? "Hide techs" : "Show techs";
      toggle.addEventListener("click", () => {
        if (open) expandedUnits.delete(stat.name);
        else expandedUnits.add(stat.name);
        renderUsage(container, rows, results, techs);
      });
      name.append(toggle);
    } else {
      name.textContent = stat.name;
    }
    row.append(
      name,
      pickBar(stat.pick_rate),
      winBar(stat.win_rate),
      el("div", "games", String(stat.picks)),
    );
    for (const cell of row.children) cell.setAttribute("role", "cell");
    const tip: Tip = {
      title: stat.name,
      lines: [
        `Fielded in ${stat.picks} of ${results} results (${pct(stat.pick_rate)})`,
        `Won ${stat.wins} of ${stat.picks} (${pct(stat.win_rate)})`,
      ],
      note:
        stat.picks < SMALL_SAMPLE
          ? `Fewer than ${SMALL_SAMPLE} games: treat the win rate as noise.`
          : undefined,
    };
    row.dataset.tip = JSON.stringify(tip);
    return unitTechs && expandedUnits.has(stat.name) ? [row, techRows(unitTechs)] : [row];
  });
  container.replaceChildren(...heads, ...body.flat());
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
      const open = el("button", "match-open") as HTMLButtonElement;
      open.type = "button";
      open.setAttribute("aria-label", `Open the match on ${when.toLocaleDateString()}`);
      const players = el("span");
      players.append(
        el("span", "won", m.winners.join(", ") || "—"),
        document.createTextNode(" beat "),
        el("span", "", m.losers.join(", ") || "—"),
      );
      open.append(
        el("span", "when", when.toLocaleDateString()),
        players,
        el("span", "meta", `${modeLabel(m.match_mode, m.game_mode)}, ${m.rounds} rounds, v${m.version}`),
      );
      open.addEventListener("click", () => showMatch(m.battle_id));
      item.append(open);
      return item;
    }),
  );
  if (matches.length === 0) {
    list.append(el("li", "empty", "No matches yet. Play a game and it will show up here."));
  }
}

/** Win rates this far from 50% get the strongest color. */
const TREND_SPAN = 0.25;
/** Equal color steps per arm of the diverging scale. */
const TREND_STEPS = 4;

function trendColor(winRate: number) {
  const delta = Math.max(-TREND_SPAN, Math.min(TREND_SPAN, winRate - 0.5));
  const step = Math.round((Math.abs(delta) / TREND_SPAN) * TREND_STEPS);
  if (step === 0) return "var(--track)";
  const pole = delta > 0 ? "var(--win)" : "var(--loss)";
  return `color-mix(in oklab, ${pole} ${(step / TREND_STEPS) * 100}%, var(--track))`;
}

function renderHeatmap(trends: VersionTrends) {
  const kind = document.querySelector<HTMLInputElement>('input[name="trend-kind"]:checked')!.value as
    | "units"
    | "specialists";
  const minGames = Number($<HTMLSelectElement>("trend-min").value);
  const rows = trends[kind];
  const grid = $("heatmap");
  grid.style.gridTemplateColumns = `minmax(110px, max-content) repeat(${trends.versions.length}, minmax(18px, 1fr))`;

  const header = [el("div", "corner", "Patch"), ...trends.versions.map((v) => {
    const col = el("div", "col", String(v.version));
    col.setAttribute("role", "columnheader");
    col.dataset.tip = JSON.stringify({
      title: `Version ${v.version}`,
      lines: [`${v.matches} matches, ${v.player_results} player results`],
    } satisfies Tip);
    return col;
  })];
  const body = rows.flatMap((row) => {
    const name = el("div", "rowname", row.name);
    name.setAttribute("role", "rowheader");
    const cells = row.cells.map((cell, i) => {
      const version = trends.versions[i];
      if (!cell) {
        return el("div", "cell none");
      }
      const winRate = cell.wins / cell.picks;
      const thin = cell.picks < minGames;
      const node = el("div", thin ? "cell thin" : "cell");
      node.setAttribute("role", "cell");
      if (!thin) {
        node.style.background = trendColor(winRate);
      }
      node.dataset.tip = JSON.stringify({
        title: `${row.name} · v${version.version}`,
        lines: [
          `Won ${cell.wins} of ${cell.picks} (${pct(winRate)})`,
          `Fielded in ${pct(cell.picks / version.player_results)} of results`,
        ],
        note: thin ? `Fewer than ${minGames} games: not colored.` : undefined,
      } satisfies Tip);
      return node;
    });
    return [name, ...cells];
  });
  grid.replaceChildren(...header, ...body);
  if (rows.length === 0) {
    grid.append(el("div", "empty", "No games match these filters yet."));
  }
}

let lastTrends: VersionTrends | null = null;

async function refreshTrends() {
  // Every patch is shown, so the patch filter doesn't apply here.
  const { min_version: _min, max_version: _max, ...filter } = currentFilter();
  lastTrends = await invoke<VersionTrends>("version_trends", { filter });
  renderHeatmap(lastTrends);
}

async function refreshStats() {
  renderedMatches = (await invoke<Status>("status")).matches;
  const filter = currentFilter();
  const [stats, techs] = await Promise.all([
    invoke<Stats>("stats", { filter }),
    invoke<UnitTechStats[]>("tech_stats", { filter }),
  ]);
  lastTechs = new Map(techs.map((t) => [t.unit, t]));
  renderUsage($("units"), stats.units, stats.player_results, lastTechs);
  renderUsage($("specialists"), stats.specialists, stats.player_results);
  $("summary").textContent =
    `${stats.matches} matches · ${stats.player_results} player results`;
  renderMatches(await invoke<MatchRow[]>("recent_matches", { limit: 15 }));
  await refreshTrends();
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

  // Events can be missed (a fast backfill may finish before this page is listening), so also
  // redraw whenever the stored data has changed since the last draw.
  if (renderedMatches >= 0 && status.matches !== renderedMatches) {
    scheduleRefresh(0);
  }

  const error = $("error");
  error.hidden = !status.last_error;
  error.textContent = status.last_error ?? "";
  return status;
}

async function showMatch(battleId: string) {
  try {
    await openMatch(battleId, facets?.main_player?.[0] ?? null, () => {
      $("view-match").hidden = true;
      $("view-stats").hidden = false;
    });
  } catch (e) {
    const error = $("error");
    error.hidden = false;
    error.textContent = String(e);
  }
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
    const target = (event.target as HTMLElement).closest<HTMLElement>("[data-tip]");
    if (!target?.dataset.tip) {
      tip.hidden = true;
      return;
    }
    const content = JSON.parse(target.dataset.tip) as Tip;
    tip.replaceChildren(el("strong", "", content.title), ...content.lines.map((l) => el("div", "", l)));
    if (content.note) {
      tip.append(el("div", "muted", content.note));
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
  const redrawHeatmap = () => lastTrends && renderHeatmap(lastTrends);
  $("trend-min").addEventListener("change", redrawHeatmap);
  for (const radio of document.querySelectorAll('input[name="trend-kind"]')) {
    radio.addEventListener("change", redrawHeatmap);
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
  // Keep the status line fresh, and catch up on any change an event didn't announce.
  window.setInterval(refreshStatus, 2000);
});
