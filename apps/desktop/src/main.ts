import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { art, artImage, loadArt, type ArtProgress } from "./art";
import { openMatch, redrawMatch } from "./match";
import { setupSharing } from "./sharing";
import { renderLoadout } from "./loadout";
import { bandLegend, deploymentPill } from "./deploy";
import { $, el, modeLabel, pct, type Tip } from "./util";

interface Status {
  upgrading: boolean;
  replay_dir: string | null;
  replay_dir_detected: boolean;
  db_path: string;
  http_url: string | null;
  matches: number;
  unfinished: number;
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
  picked_specialists: UsageStat[];
  granted_specialists: UsageStat[];
  vs_rating: RatingBand[];
}

interface RatingBand {
  name: string;
  from: number | null;
  to: number | null;
  games: number;
  wins: number;
  win_rate: number;
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
  map: string;
  winners: string[];
  losers: string[];
  winner_elos: (number | null)[];
  loser_elos: (number | null)[];
  /** Per player, in the same order as the names. */
  winner_packs: (StarterPack | null)[];
  loser_packs: (StarterPack | null)[];
  /** Each player's round 1 deployment, which classes the match for them, when classified. */
  winner_deployments: (string | null)[];
  loser_deployments: (string | null)[];
}

/** Whole-match stats (mechabellum_server::GameStats). */
interface GameStats {
  matches: number;
  player_results: number;
  lengths: [number, number][];
  /** Per round 1 deployment: matches are classed by round 1's layout. */
  deployments: { label: string; matches: number; wins: number; round_one_wins: number }[];
  /** 1v1 matches by both players' round 1 deployments. */
  matchups: { deployment: string; against: string; matches: number; wins: number }[];
  winners: ArmyMeans | null;
  losers: ArmyMeans | null;
  units: {
    name: string;
    games: number;
    wins: number;
    use_rate: number;
    win_rate: number;
    kept: number;
    added: number;
    sold: number;
    mean_peak: number;
    mean_max_level: number;
  }[];
}

interface ArmyMeans {
  results: number;
  final_units: number;
  peak_units: number;
  mean_level: number;
  max_level: number;
  level_ups: number;
  units_added: number | null;
  units_sold: number | null;
  techs: number;
  cards: number;
}

/** Units a player's starter pack put on the board for round 1. */
interface StarterPack {
  id: string;
  units: { unit_id: number; name: string; count: number }[];
}

interface TrendCell {
  picks: number;
  wins: number;
}

interface VersionTrends {
  versions: { label: string; matches: number; player_results: number }[];
  units: { name: string; picks: number; cells: (TrendCell | null)[] }[];
  specialists: { name: string; picks: number; cells: (TrendCell | null)[] }[];
}

interface TechStat {
  tech: string;
  tech_id: string | null;
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
  opponents_of?: string;
  include_offline?: boolean;
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
let lastStats: Stats | null = null;
let lastGames: GameStats | null = null;

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
  const view = $<HTMLSelectElement>("filter-view").value;
  const me = facets?.main_player?.[0];
  if (me && view === "me") filter.player_id = me;
  if (me && view === "opponents") filter.opponents_of = me;
  if ($<HTMLInputElement>("filter-offline").checked) filter.include_offline = true;
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
  // Start on your own side: this is your stats app. Without a known player, show everyone.
  const view = $<HTMLSelectElement>("filter-view");
  const meOption = $<HTMLOptionElement>("view-me");
  meOption.textContent = facets.main_player ? `Me (${facets.main_player[1]})` : "Me";
  for (const option of view.options) option.disabled = !facets.main_player && option.value !== "all";
  if (first) view.value = facets.main_player ? "me" : "all";
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

/** Small art beside a name, when extracted. */
function nameIcon(src: string | null) {
  return artImage(src, "name-icon") ?? el("span", "name-icon placeholder");
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
    const techName = el("div", "name");
    const icon = (t.tech_id && art.techIcon(t.tech_id)) || art.techIconByName(t.tech);
    techName.append(nameIcon(icon), el("span", "", t.tech));
    row.append(techName, pickBar(t.pick_rate), winCell, el("div", "games", String(t.picks)));
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

type SortKey = "name" | "pick" | "win" | "games";
/** Each table's sort: column and direction. Clicking a header sorts by it; again reverses. */
const usageSort: Record<"unit" | "specialist", { key: SortKey; desc: boolean }> = {
  unit: { key: "pick", desc: true },
  specialist: { key: "pick", desc: true },
};

function sortUsage(rows: UsageStat[], key: SortKey, desc: boolean): UsageStat[] {
  const dir = desc ? -1 : 1;
  const value = (s: UsageStat) =>
    key === "win" ? s.win_rate : key === "pick" ? s.pick_rate : key === "games" ? s.picks : 0;
  return [...rows].sort((a, b) => {
    if (key === "name") return dir * a.name.localeCompare(b.name);
    // Ties go to the one with more games.
    return dir * (value(a) - value(b)) || b.picks - a.picks || a.name.localeCompare(b.name);
  });
}

function renderUsage(
  container: HTMLElement,
  unsorted: UsageStat[],
  results: number,
  kind: "unit" | "specialist",
  techs?: Map<string, UnitTechStats>,
) {
  const icon = (name: string) =>
    nameIcon(kind === "unit" ? art.unitIconByName(name) : art.specialistByName(name));
  const sort = usageSort[kind];
  const rows = sortUsage(unsorted, sort.key, sort.desc);
  const heads = (
    [
      ["", "Name", "name"],
      ["", "Pick rate", "pick"],
      ["", "Win rate", "win"],
      ["num", "Games", "games"],
    ] as [string, string, SortKey][]
  ).map(([cls, text, key]) => {
    const head = el("div", `head ${cls}`);
    head.setAttribute("role", "columnheader");
    const active = sort.key === key;
    head.setAttribute("aria-sort", active ? (sort.desc ? "descending" : "ascending") : "none");
    const button = el("button", `sort${active ? " active" : ""}`, text) as HTMLButtonElement;
    button.type = "button";
    button.title = `Sort by ${text.toLowerCase()}`;
    if (active) button.append(el("span", "sort-arrow", sort.desc ? " ▾" : " ▴"));
    button.addEventListener("click", () => {
      // Names read best A to Z first; numbers highest first.
      usageSort[kind] = active ? { key, desc: !sort.desc } : { key, desc: key !== "name" };
      renderUsage(container, unsorted, results, kind, techs);
    });
    head.append(button);
    return head;
  });
  const body = rows.map((stat) => {
    const row = el("div", stat.picks < SMALL_SAMPLE ? "row small" : "row");
    row.setAttribute("role", "row");
    const unitTechs = techs?.get(stat.name);
    const name = el("div", "name");
    if (unitTechs) {
      const open = expandedUnits.has(stat.name);
      const toggle = el("button", "disclosure") as HTMLButtonElement;
      toggle.append(icon(stat.name), el("span", "", stat.name));
      toggle.type = "button";
      toggle.setAttribute("aria-expanded", String(open));
      toggle.title = open ? "Hide techs" : "Show techs";
      toggle.addEventListener("click", () => {
        if (open) expandedUnits.delete(stat.name);
        else expandedUnits.add(stat.name);
        renderUsage(container, unsorted, results, kind, techs);
      });
      name.append(toggle);
    } else {
      name.append(icon(stat.name), el("span", "", stat.name));
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

/** Win rate against opponents rated lower, about the same, and higher. */
function renderRatingBands(bands: RatingBand[]) {
  const container = $("vs-rating");
  const rated = bands.reduce((sum, b) => sum + b.games, 0);
  const heads = [
    ["", "Opponent"],
    ["", "Share of games"],
    ["", "Win rate"],
    ["num", "Games"],
  ].map(([cls, text]) => {
    const head = el("div", `head ${cls}`, text);
    head.setAttribute("role", "columnheader");
    return head;
  });
  const rows = bands.map((band) => {
    const row = el("div", band.games < SMALL_SAMPLE ? "row small" : "row");
    row.setAttribute("role", "row");
    row.append(
      el("div", "name", band.name),
      pickBar(rated ? band.games / rated : 0),
      band.games ? winBar(band.win_rate) : el("div", "muted", "No games"),
      el("div", "games", String(band.games)),
    );
    for (const cell of row.children) cell.setAttribute("role", "cell");
    if (band.games) {
      const tip: Tip = {
        title: band.name,
        lines: [`Won ${band.wins} of ${band.games} (${pct(band.win_rate)})`],
        note: band.games < SMALL_SAMPLE ? `Fewer than ${SMALL_SAMPLE} games: treat the win rate as noise.` : undefined,
      };
      row.dataset.tip = JSON.stringify(tip);
    }
    return row;
  });
  container.replaceChildren(...heads, ...rows);
  $("vs-rating-note").textContent =
    rated === 0
      ? "Ratings come from the game's log while this app runs, so only games played since then have them."
      : `${rated} rated ${rated === 1 ? "game" : "games"}. Ratings come from the game's log while this app runs.`;
}

let allMatches: MatchRow[] = [];
/** The full list is only fetched once the Matches tab is opened. */
let matchListLoaded = false;

async function refreshMatchList() {
  matchListLoaded = true;
  allMatches = await invoke<MatchRow[]>("recent_matches", { limit: 5000 });
  renderMatchList();
}

function renderMatchList() {
  const me = facets?.main_player?.[1] ?? null;
  const query = $<HTMLInputElement>("match-search").value.trim().toLowerCase();
  const shown = allMatches.filter((m) => {
    if (!query) return true;
    const packUnits = [...m.winner_packs, ...m.loser_packs].flatMap((p) => p?.units.map((u) => u.name) ?? []);
    const deployments = [...(m.winner_deployments ?? []), ...(m.loser_deployments ?? [])].filter((d): d is string => !!d);
    const text = [...m.winners, ...m.losers, ...packUnits, ...deployments, m.map, modeLabel(m.match_mode, m.game_mode), String(m.version)]
      .join(" ")
      .toLowerCase();
    return query.split(/\s+/).every((word) => text.includes(word));
  });
  $("matches-summary").textContent = query
    ? `${shown.length} of ${allMatches.length} matches`
    : `${allMatches.length} matches`;

  const list = $("match-list");
  list.replaceChildren(
    ...shown.slice(0, 400).map((m) => {
      const item = el("li", "match-row");
      const when = new Date(m.occurred_on);
      const iWon = me !== null && m.winners.includes(me);
      const iLost = me !== null && m.losers.includes(me);
      const outcome = el("span", `outcome ${iWon ? "won" : iLost ? "lost" : "other"}`, iWon ? "Won" : iLost ? "Lost" : "");
      // One line per player, winners first, each with the starter pack they opened with.
      const players = el("span", "players");
      const line = (name: string, elo: number | null, deployment: string | null, pack: StarterPack | null, won: boolean) => {
        const row = el("span", `player-line${won ? " winner" : ""}`);
        row.append(el("span", "result-tag", won ? "Won" : "Lost"));
        const who = el("span", "name", name);
        if (elo != null) {
          const badge = el("span", "elo", String(elo));
          badge.title = "MMR at match start";
          who.append(badge);
        }
        row.append(who);
        // How they deployed for round 1, which classes the match; an empty cell keeps packs in a column.
        row.append(deployment ? deploymentPill(deployment) : el("span"));
        if (pack) row.append(packChips(pack));
        return row;
      };
      m.winners.forEach((name, i) => players.append(line(name, m.winner_elos[i], m.winner_deployments?.[i] ?? null, m.winner_packs[i], true)));
      m.losers.forEach((name, i) => players.append(line(name, m.loser_elos[i], m.loser_deployments?.[i] ?? null, m.loser_packs[i], false)));
      if (players.children.length === 0) players.append(el("span", "player-line muted", "No result recorded"));
      const details = el("span", "details", `${m.map}, ${modeLabel(m.match_mode, m.game_mode)}, ${m.rounds} rounds, version ${m.version}`);
      const open = el("button", "open-board", "Open board") as HTMLButtonElement;
      open.type = "button";
      open.setAttribute("aria-label", `Open the board for ${m.winners.join(", ")} vs ${m.losers.join(", ")} on ${when.toLocaleDateString()}`);
      open.addEventListener("click", () => showMatch(m.battle_id));
      item.append(el("span", "when", when.toLocaleDateString()), outcome, players, details, open);
      item.addEventListener("dblclick", () => showMatch(m.battle_id));
      return item;
    }),
  );
  if (allMatches.length === 0) {
    list.append(el("li", "empty", "No matches yet. Play a game and it will show up here."));
  } else if (shown.length === 0) {
    list.append(el("li", "empty", "No matches fit that search."));
  } else if (shown.length > 400) {
    list.append(el("li", "empty", `Showing the newest 400. Search to narrow the rest.`));
  }
}

/** A starter pack as small unit icons with counts, e.g. [Fang] 3 [Steel Ball] 2. */
function packChips(pack: StarterPack) {
  const chips = el("span", "pack");
  const tip: Tip = {
    title: "Starter pack",
    lines: pack.units.map((u) => `${u.count} ${u.name}`),
    note: "The units it put on the board for round 1. The starting specialist is picked separately.",
  };
  chips.dataset.tip = JSON.stringify(tip);
  for (const u of pack.units) {
    const chip = el("span", "pack-unit");
    chip.append(artImage(art.unitIcon(u.unit_id), "pack-icon", u.name) ?? el("span", "pack-icon placeholder", u.name.slice(0, 3)));
    chip.append(el("span", "pack-count", `×${u.count}`));
    chips.append(chip);
  }
  return chips;
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

  const byMonth = $<HTMLSelectElement>("trend-period").value === "month";
  const columnName = (label: string) => {
    if (!byMonth) return `Build ${label}`;
    const [year, month] = label.split("-").map(Number);
    return new Date(year, month - 1, 1).toLocaleDateString(undefined, { month: "long", year: "numeric" });
  };
  const header = [el("div", "corner", byMonth ? "Month" : "Build"), ...trends.versions.map((v) => {
    const col = el("div", "col", byMonth ? v.label.replace(/^20(\d\d)-/, "$1/") : v.label);
    col.setAttribute("role", "columnheader");
    col.dataset.tip = JSON.stringify({
      title: columnName(v.label),
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
        title: `${row.name}, ${columnName(version.label)}`,
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

/** Win rate by how round 1 was deployed: the layout a match is classed by. */
function renderDeployments(g: GameStats) {
  $("deploy-legend").replaceChildren(bandLegend());
  const container = $("deployments");
  const heads = ["Round 1 deployment", "Matches", "Match win rate", "Won round 1"].map((h, i) =>
    el("div", `head${i === 0 || i === 2 ? "" : " num"}`, h),
  );
  const rows = g.deployments.flatMap((d) => {
    const small = d.matches < SMALL_SAMPLE;
    const cells = [
      deploymentPill(d.label),
      el("div", "num", String(d.matches)),
      winBar(d.matches ? d.wins / d.matches : 0.5),
      el("div", "num", d.matches ? pct(d.round_one_wins / d.matches) : "–"),
    ];
    const tip: Tip = {
      title: `${d.label} in round 1`,
      lines: [`Won ${d.wins} of ${d.matches} matches`, `Won round 1 itself in ${d.round_one_wins}`],
      note: small ? `Fewer than ${SMALL_SAMPLE} matches: treat the win rate as noise.` : undefined,
    };
    for (const c of cells) {
      if (small) c.classList.add("muted");
      c.dataset.tip = JSON.stringify(tip);
    }
    return cells;
  });
  container.replaceChildren(...heads, ...rows);
  if (rows.length === 0) container.append(el("div", "empty", "No classified round 1 deployments fit these filters. Classes cover 1v1 on the 2.0 board."));
}

const DEPLOYMENT_ORDER = ["Sym aggressive", "Asym aggressive", "Standard", "Sym midboard", "Asym midboard", "Backboard"];

function renderMatchups(g: GameStats) {
  const container = $("matchups");
  // Rows and columns differ in the Me and My opponents views: your deployments against theirs.
  const rowsPresent = DEPLOYMENT_ORDER.filter((l) => g.matchups.some((m) => m.deployment === l));
  const colsPresent = DEPLOYMENT_ORDER.filter((l) => g.matchups.some((m) => m.against === l));
  container.style.gridTemplateColumns = `minmax(120px, auto) repeat(${colsPresent.length}, minmax(52px, 1fr))`;
  const cells: HTMLElement[] = [el("div", "corner", "Row vs column")];
  for (const l of colsPresent) cells.push(el("div", "col-head", l));
  for (const row of rowsPresent) {
    cells.push(deploymentPill(row));
    for (const col of colsPresent) {
      const m = g.matchups.find((x) => x.deployment === row && x.against === col);
      const cell = el("div", "cell");
      if (m) {
        const r = m.wins / m.matches;
        cell.textContent = pct(r);
        if (m.matches < SMALL_SAMPLE) cell.classList.add("thin");
        else cell.style.background = trendColor(r);
        const tip: Tip = { title: `Round 1: ${row} against ${col}`, lines: [`Won ${m.wins} of ${m.matches} matches (${pct(r)})`] };
        cell.dataset.tip = JSON.stringify(tip);
      }
      cells.push(cell);
    }
  }
  container.replaceChildren(...cells);
  $("matchups-note").textContent = rowsPresent.length
    ? `1v1 matches by both players' round 1 layouts. Striped: fewer than ${SMALL_SAMPLE} matches.`
    : "No 1v1 matches with both round 1 layouts classed fit these filters.";
}

/** Two columns compared row by row, the larger value bright: the tale of the tape. */
function taleRows(
  container: HTMLElement,
  heads: [string, string],
  rows: [string, number | null | undefined, number | null | undefined, number][],
) {
  const cells: HTMLElement[] = [el("div", "tale-head left", heads[0]), el("div"), el("div", "tale-head right", heads[1])];
  for (const [label, a, b, digits] of rows) {
    if (a == null || b == null) continue;
    const max = Math.max(a, b) || 1;
    const track = (v: number, hi: boolean, side: string) => {
      const t = el("div", `tale-track ${side}`);
      const fill = el("span", hi ? "hi" : "");
      fill.style.width = `${(v / max) * 100}%`;
      t.append(fill);
      return t;
    };
    cells.push(
      el("div", `tale-value left${a > b ? " hi" : ""}`, a.toFixed(digits)),
      track(a, a > b, "left"),
      el("div", "tale-label", label),
      track(b, b > a, "right"),
      el("div", `tale-value${b > a ? " hi" : ""}`, b.toFixed(digits)),
    );
  }
  container.replaceChildren(...cells);
}

function renderArmy(g: GameStats) {
  const w = g.winners;
  const l = g.losers;
  if (!w || !l) {
    $("army").replaceChildren(el("div", "empty", "Needs matches with both a winner and a loser."));
    return;
  }
  taleRows($("army"), [`Winners (${w.results})`, `Losers (${l.results})`], [
    ["Final army", w.final_units, l.final_units, 1],
    ["Peak army", w.peak_units, l.peak_units, 1],
    ["Mean level", w.mean_level, l.mean_level, 1],
    ["Top level", w.max_level, l.max_level, 1],
    ["Level-ups", w.level_ups, l.level_ups, 1],
    ["Units added", w.units_added, l.units_added, 1],
    ["Units sold", w.units_sold, l.units_sold, 1],
    ["Techs", w.techs, l.techs, 1],
    ["Cards picked", w.cards, l.cards, 1],
  ]);
}

function renderLengths(g: GameStats) {
  const container = $("lengths");
  const max = Math.max(1, ...g.lengths.map(([, n]) => n));
  const total = g.lengths.reduce((sum, [, n]) => sum + n, 0);
  container.replaceChildren(
    ...g.lengths.map(([fights, n]) => {
      const col = el("div", "length-col");
      const bar = el("span", "length-bar");
      bar.style.height = `${(n / max) * 100}%`;
      col.append(el("span", "length-count", String(n)), el("span", "length-plot"), el("span", "length-fights", String(fights)));
      col.children[1].append(bar);
      const tip: Tip = { title: `${fights} fights`, lines: [`${n} of ${total} matches (${pct(n / total)})`] };
      col.dataset.tip = JSON.stringify(tip);
      return col;
    }),
  );
  if (g.lengths.length === 0) container.append(el("div", "empty", "No matches fit these filters yet."));
}

function renderUsed(g: GameStats) {
  const container = $("used");
  const heads = ["Unit", "Used", "Win rate", "Kept", "Sold", "Top level"].map((h, i) => el("div", `head${i === 0 || i === 2 ? "" : " num"}`, h));
  const rows = g.units.slice(0, 40).flatMap((u) => {
    const name = el("div", "name");
    name.append(nameIcon(art.unitIconByName(u.name)), el("span", "", u.name));
    const cells = [
      name,
      el("div", "num", pct(u.use_rate)),
      winBar(u.win_rate),
      el("div", "num", pct(u.games ? u.kept / u.games : 0)),
      el("div", "num", `${u.sold} of ${u.added}`),
      el("div", "num", u.mean_max_level.toFixed(1)),
    ];
    const tip: Tip = {
      title: u.name,
      lines: [
        `Fielded at some point in ${u.games} of ${g.player_results} results; won ${u.wins}`,
        `Still fielded in the last fight in ${u.kept} of them`,
        `Up to ${u.mean_peak.toFixed(1)} copies at once on average; ${u.sold} of ${u.added} added copies sold`,
      ],
      note: u.games < SMALL_SAMPLE ? `Fewer than ${SMALL_SAMPLE} games: treat the win rate as noise.` : undefined,
    };
    for (const c of cells) {
      if (u.games < SMALL_SAMPLE) c.classList.add("muted");
      c.dataset.tip = JSON.stringify(tip);
    }
    return cells;
  });
  container.replaceChildren(...heads, ...rows);
  if (rows.length === 0) container.append(el("div", "empty", "No games match these filters yet."));
}

function renderGameStats(g: GameStats) {
  renderDeployments(g);
  renderMatchups(g);
  renderArmy(g);
  renderLengths(g);
  renderUsed(g);
}

async function refreshTrends() {
  // Every patch is shown, so the patch filter doesn't apply here.
  const { min_version: _min, max_version: _max, ...filter } = currentFilter();
  const period = $<HTMLSelectElement>("trend-period").value;
  lastTrends = await invoke<VersionTrends>("version_trends", { filter: { ...filter, period } });
  renderHeatmap(lastTrends);
}

async function refreshStats() {
  renderedMatches = (await invoke<Status>("status")).matches;
  const filter = currentFilter();
  const stats = invoke<Stats>("stats", { filter }).then((stats) => {
    lastStats = stats;
    renderUsage($("units"), stats.units, stats.player_results, "unit", lastTechs);
    renderSpecialists();
    renderRatingBands(stats.vs_rating);
    $("summary").textContent = `${stats.matches} matches · ${stats.player_results} player results`;
  });
  // Tech breakdowns arrive separately and fill in the unit rows' disclosure.
  const techs = invoke<UnitTechStats[]>("tech_stats", { filter }).then(async (techs) => {
    lastTechs = new Map(techs.map((t) => [t.unit, t]));
    await stats;
    if (lastStats) renderUsage($("units"), lastStats.units, lastStats.player_results, "unit", lastTechs);
  });
  const trends = refreshTrends();
  const games = invoke<GameStats>("game_stats", { filter }).then((g) => {
    lastGames = g;
    renderGameStats(g);
  });
  const matches = matchListLoaded ? refreshMatchList() : Promise.resolve();
  await Promise.all([stats, techs, trends, games, matches]);

}

async function refreshStatus(): Promise<Status> {
  const status = await invoke<Status>("status");
  $("replay-dir").textContent = status.replay_dir ?? "not found";
  $("replay-dir").title = status.replay_dir ?? "";
  $("replay-dir-badge").hidden = !status.replay_dir_detected;
  $("auto-dir").hidden = status.replay_dir_detected || !status.replay_dir;

  const { done, total, finished } = status.backfill;
  const parts = [`${status.matches} matches stored`];
  if (status.unfinished > 0) {
    parts.push(`${status.unfinished} ended before the first fight and aren't counted`);
  }
  if (status.upgrading) parts.unshift("Updating stats for this version…");
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

type View = "stats" | "matches" | "match" | "sharing" | "loadout";

function showView(view: View) {
  if (view === "matches" && !matchListLoaded) {
    $("match-list").replaceChildren(el("li", "empty", "Loading matches…"));
    refreshMatchList();
  }
  $("view-stats").hidden = view !== "stats";
  $("view-matches").hidden = view !== "matches";
  $("view-match").hidden = view !== "match";
  $("view-sharing").hidden = view !== "sharing";
  $("view-loadout").hidden = view !== "loadout";
  if (view === "loadout") renderLoadout(facets?.main_player?.[0] ?? null);
  for (const tab of document.querySelectorAll<HTMLElement>(".tab")) {
    const current = tab.dataset.view === view || (view === "match" && tab.dataset.view === "matches");
    if (current) tab.setAttribute("aria-current", "page");
    else tab.removeAttribute("aria-current");
  }
  window.scrollTo(0, 0);
}

async function showMatch(battleId: string) {
  try {
    await openMatch(battleId, facets?.main_player?.[0] ?? null, () => showView("matches"));
    showView("match");
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
    if (matchListLoaded) await refreshMatchList();
  }, delay);
}

/** Specialists held at the end: starting ones, picks, and Research Center grants alike. */
function renderSpecialists() {
  if (!lastStats) return;
  renderUsage($("specialists"), lastStats.specialists, lastStats.player_results, "specialist");
}

function applyArt() {
  const background = art.background();
  const backdrop = $("backdrop");
  backdrop.style.backgroundImage = background ? `url("${background}")` : "";
  document.body.classList.toggle("has-art", background !== null);

  const status = art.status();
  const line = $("art-status");
  line.hidden = !status;
  line.textContent = status ?? "";

  if (lastStats) {
    renderUsage($("units"), lastStats.units, lastStats.player_results, "unit", lastTechs);
    renderSpecialists();
  }
  if (lastGames) renderUsed(lastGames);
  if (matchListLoaded) renderMatchList();
  redrawMatch();
}

/** The first-run splash: a progress bar, the steps with the current one marked, and a line
 * about what's happening now. Hidden when extraction isn't running. */
function showArtProgress(progress: ArtProgress | null) {
  const splash = $("art-splash");
  splash.hidden = progress === null;
  if (!progress) return;
  const percent = Math.round(Math.min(1, Math.max(0, progress.fraction)) * 100);
  $("splash-fill").style.width = `${percent}%`;
  $("splash-bar").setAttribute("aria-valuenow", String(percent));
  $("splash-percent").textContent = `${percent}%`;
  const list = $("splash-steps");
  if (list.children.length !== progress.steps.length) {
    list.replaceChildren(...progress.steps.map((step) => el("li", "", step)));
  }
  [...list.children].forEach((item, i) => {
    (item as HTMLElement).dataset.state = i < progress.step ? "done" : i === progress.step ? "active" : "";
  });
  $("splash-detail").textContent = progress.detail;
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

const PALETTE_KEY = "palette";

/** The color pair for you and your opponent, remembered on this computer. */
function applyPalette(value: string) {
  if (value === "game") document.documentElement.removeAttribute("data-palette");
  else document.documentElement.dataset.palette = value;
}

function setupPalette() {
  let saved = "game";
  try {
    saved = localStorage.getItem(PALETTE_KEY) ?? "game";
  } catch {
    /* Storage can be unavailable; the default still works. */
  }
  const select = $<HTMLSelectElement>("palette");
  if (![...select.options].some((o) => o.value === saved)) saved = "game";
  select.value = saved;
  applyPalette(saved);
  select.addEventListener("change", () => {
    applyPalette(select.value);
    try {
      localStorage.setItem(PALETTE_KEY, select.value);
    } catch {
      /* Not remembered, but applied for now. */
    }
    // The heatmap computes its colors in script; redraw it.
    if (lastTrends) renderHeatmap(lastTrends);
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  setupPalette();
  setupTooltip();
  for (const tab of document.querySelectorAll<HTMLElement>(".tab")) {
    tab.addEventListener("click", () => showView(tab.dataset.view as View));
  }
  $("match-search").addEventListener("input", () => renderMatchList());
  loadArt(applyArt, showArtProgress);

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
  for (const id of ["filter-version", "filter-mode", "filter-view", "filter-offline"]) {
    $(id).addEventListener("change", () => refreshStats());
  }
  const redrawHeatmap = () => lastTrends && renderHeatmap(lastTrends);
  $("trend-min").addEventListener("change", redrawHeatmap);
  $("trend-period").addEventListener("change", () => refreshTrends());
  for (const radio of document.querySelectorAll('input[name="trend-kind"]')) {
    radio.addEventListener("change", redrawHeatmap);
  }

  await listen<string>("stats-upgrade", (event) => {
    if (event.payload === "done") scheduleRefresh(0);
    refreshStatus();
  });
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
  await setupSharing();
  // Keep the status line fresh, and catch up on any change an event didn't announce.
  window.setInterval(refreshStatus, 2000);
  // A window hidden in the tray may not run timers; catch up as soon as it's shown.
  window.addEventListener("focus", () => refreshStatus());
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refreshStatus();
  });
});
