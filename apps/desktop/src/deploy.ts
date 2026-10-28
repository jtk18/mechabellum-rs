// Deployment classes (mechabellum::classify) and whole-match summaries (mechabellum::PlayerGame).
import { el, pct, type Tip } from "./util";

export interface Deployment {
  /** One of DEPLOYMENTS. */
  label: string;
  depth: "aggressive" | "midboard" | "backboard";
  /** How the army leans, -1 all on the player's own left, 1 all on their right: an aggressive army's units in that band, a midboard one's whole army. Null for backboard. */
  lean: number | null;
  /** Share of the army in each band, front to back, by footprint area. */
  bands: [number, number, number, number];
  flank: number;
}

export interface FightSummary {
  fight: number;
  result: string;
  hp: number;
  units: number;
  /** Supply the army was worth: each unit's sell value as the game records it. Null if a unit's isn't known. */
  value: number | null;
  deployment: Deployment | null;
  /** In a 1v1, how both players' deployments met when both were one-sided. */
  meeting: Meeting | null;
  /** [unit_id, x, y, width, depth, level, band] */
  board: number[][];
}

export interface UnitUse {
  unit_id: number;
  name: string;
  /** Null when the match's units added and sold are (a replay that starts mid-match). */
  added: number | null;
  sold: number | null;
  peak: number;
  last: number;
  max_level: number;
  first_fight: number;
}

export interface PlayerGame {
  half: "south" | "north" | null;
  fights_won: number;
  final_units: number;
  peak_units: number;
  final_value: number | null;
  peak_value: number | null;
  mean_level: number;
  max_level: number;
  /** Null when the replay doesn't run from fight 1 (as for units_added). */
  level_ups: number | null;
  units_added: number | null;
  units_sold: number | null;
  techs: number;
  cards: number | null;
  /** Round 1's deployment label, which classes the match; null when round 1 isn't recorded. */
  opening: string | null;
  /** How round 1's deployments met. */
  opening_meeting: Meeting | null;
  units: UnitUse[];
  fights: FightSummary[];
}

export const BANDS = ["Aggressive", "Midboard", "Backboard"];

/** Every deployment class, front to back (mechabellum::Deployment::label). */
export const DEPLOYMENTS = ["Sym aggressive", "Asym aggressive", "Standard", "Asym mid aggro", "Backboard"];

/** How two one-sided 1v1 deployments meet (mechabellum::Meeting). */
export type Meeting = "Asym headbutt" | "Asym crossfire";

export const MEETING_TIP: Tip = {
  title: "When both go asym",
  lines: [
    "Asym headbutt: both armies grouped on the same lane of the board, so they meet head on.",
    "Asym crossfire: grouped on opposite lanes, so each faces the other's empty side.",
  ],
  note: "A lane is the board's east or west half; one player's left is the other's right.",
};

/** How classes are decided, for tooltips. Mirrors the constants in crates/mechabellum/src/replays/summary.rs. */
export const RULES: Tip = {
  title: "Deployment classes",
  lines: [
    "Aggressive: front edge within 5 squares of the zone's front edge, or in a flank column.",
    "Midboard: from the aggressive band back to the towers' centers.",
    "Backboard: behind the towers' centers.",
    "Each unit counts by the ground it covers. An army is aggressive or backboard when that band holds the most of its ground and at least 40%; otherwise it's midboard.",
    "A midboard army is Standard when its ground is spread across both sides, and Asym mid aggro when three quarters or more of it stands on one side. An aggressive army is sym or asym by its aggressive ground the same way. Backboard isn't split.",
    "Two asym armies on the same lane are an asym headbutt.",
  ],
  note: "A match is classed by round 1's layout; later fights are classed the same way to show how the board grew. 1v1 on the 2.0 board, build 2259 on.",
};

/** The band of a deployment label: Standard and Asym mid aggro are midboard, both aggressive
 * classes aggressive. -1 when unknown. */
export function labelBand(label: string | null | undefined) {
  if (!label) return -1;
  if (label === "Standard" || label === "Asym mid aggro") return 1;
  const band = label.replace(/^(Sym|Asym) /, "").toLowerCase();
  return BANDS.findIndex((b) => b.toLowerCase() === band);
}

/** A small tag for a headbutt or crossfire, with the explanation on hover. */
export function meetingTag(meeting: Meeting | null | undefined) {
  if (!meeting) return null;
  const tag = el("span", `meeting-tag${meeting === "Asym headbutt" ? " headbutt" : ""}`, meeting);
  tag.dataset.tip = JSON.stringify(MEETING_TIP);
  return tag;
}

/** A band's color; unclassified marks get the muted ink (3:1 on the panel), not the near-invisible track. */
export const bandColor = (band: number) => (band >= 0 && band < BANDS.length ? `var(--band-${band})` : "var(--text-muted)");

/** A swatch and a label, e.g. ■ Standard. */
export function deploymentPill(label: string | null | undefined) {
  const pill = el("span", `deploy-pill${label ? "" : " none"}`);
  const swatch = el("span", "deploy-swatch");
  swatch.style.background = bandColor(labelBand(label));
  pill.append(swatch, document.createTextNode(label ?? "Not classed"));
  pill.dataset.tip = JSON.stringify(RULES);
  return pill;
}

/** The army's share in each band as one stacked bar. */
export function shareBar(bands: number[]) {
  const bar = el("span", "share-bar");
  const tip: Tip = {
    title: "Where the army stood",
    lines: BANDS.map((b, i) => `${b}: ${pct(bands[i])}`),
    note: "Share of the ground the army covered, front to back.",
  };
  bar.dataset.tip = JSON.stringify(tip);
  bands.forEach((share, i) => {
    if (share <= 0) return;
    const part = el("span");
    part.style.flex = `${share} 0 0`;
    part.style.background = bandColor(i);
    bar.append(part);
  });
  return bar;
}

/** The band colors with their names, and the rules on hover. */
export function bandLegend() {
  const legend = el("span", "deploy-legend");
  legend.dataset.tip = JSON.stringify(RULES);
  BANDS.forEach((name, i) => {
    const swatch = el("span", "deploy-swatch");
    swatch.style.background = bandColor(i);
    legend.append(swatch, el("span", "", name));
  });
  return legend;
}

/** What each tale-of-the-tape row measures (mechabellum::PlayerGame), for its tooltip. */
export const MEASURES: Record<string, string> = {
  "Fights won": "Fights won over the match, the last one included. A draw, where every unit dies, counts as a win for both players.",
  "Final army": "Units fielded in the last fight.",
  "Peak army": "Most units fielded in any one fight.",
  "Army value": "Supply the last fight's army was worth: each unit's sell value as the game records it, discounts and half-price free units included. Upgrades and techs aren't in it. Units bought in the last round take the price their other copies in the match show, else the unit's list price; an army with a unit no price is known for is left out.",
  "Peak army value": "The most supply any one fight's army was worth, by the same measure.",
  "Mean level": "Average level of the units in the last fight. Level 1 is a unit never upgraded.",
  "Top level": "Highest level of any unit in the last fight.",
  "Level-ups": "Unit upgrades bought over the match, units later sold included. Levels units arrive with, from cards or techs, don't count.",
  "Units added": "Units that joined the army over the match: the starter pack, purchases, and units from cards.",
  "Units sold": "Units in one fight's army and gone from the next: sold, or recycled by a skill.",
  Techs: "Techs researched by the last fight, its own round's research included, across all unit types.",
  "Cards picked": "Cards picked from the offers between rounds: specialists, unit reinforcements, skills and items.",
};

/** Two columns compared row by row, the larger value bright, each row explained on hover. */
export function renderTale(
  container: HTMLElement,
  heads: [HTMLElement, HTMLElement],
  names: [string, string],
  rows: [string, number | null | undefined, number | null | undefined, number, string?][],
  note?: string,
) {
  heads[0].classList.add("tale-head", "left");
  heads[1].classList.add("tale-head", "right");
  const cells: HTMLElement[] = [heads[0], el("div"), heads[1]];
  for (const [label, a, b, digits, over] of rows) {
    if (a == null || b == null) continue;
    const max = Math.max(a, b) || 1;
    const track = (v: number, hi: boolean, side: string) => {
      const t = el("div", `tale-track ${side}`);
      const fill = el("span", hi ? "hi" : "");
      fill.style.width = `${(v / max) * 100}%`;
      t.append(fill);
      return t;
    };
    const name = el("div", "tale-label", label);
    // A row averaged over fewer results than the header says shows how many.
    if (over) name.append(el("span", "tale-sub", over));
    // One keyboard stop per row, so the explanation is reachable without a mouse.
    name.tabIndex = 0;
    const fmt = (v: number) => v.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
    const row = [
      el("div", `tale-value left${a > b ? " hi" : ""}`, fmt(a)),
      track(a, a > b, "left"),
      name,
      track(b, b > a, "right"),
      el("div", `tale-value${b > a ? " hi" : ""}`, fmt(b)),
    ];
    const tip: Tip = {
      title: label,
      lines: [MEASURES[label] ?? "", `${names[0]}: ${fmt(a)}`, `${names[1]}: ${fmt(b)}`, ...(over ? [`Averaged over ${over} results.`] : [])],
      note,
    };
    // The whole row answers the hover, not just the label.
    for (const cell of row) cell.dataset.tip = JSON.stringify(tip);
    cells.push(...row);
  }
  container.replaceChildren(...cells);
}
