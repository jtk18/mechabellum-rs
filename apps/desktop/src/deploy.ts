// Deployment classes (mechabellum::classify) and whole-match summaries (mechabellum::PlayerGame).
import { el, pct, type Tip } from "./util";

export interface Deployment {
  /** "Sym aggressive", "Asym aggressive", "Standard", "Sym midboard", "Asym midboard" or "Backboard". */
  label: string;
  depth: "aggressive" | "standard" | "midboard" | "backboard";
  /** Aggressive and midboard only: how that band's units lean, -1 all on the player's own left, 1 all on their right. */
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
  deployment: Deployment | null;
  /** [unit_id, x, y, width, depth, level, band] */
  board: number[][];
}

export interface UnitUse {
  unit_id: number;
  name: string;
  added: number;
  sold: number;
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
  mean_level: number;
  max_level: number;
  level_ups: number;
  units_added: number | null;
  units_sold: number | null;
  techs: number;
  cards: number;
  /** Round 1's deployment label, which classes the match; null when round 1 isn't recorded. */
  opening: string | null;
  units: UnitUse[];
  fights: FightSummary[];
}

export const BANDS = ["Aggressive", "Standard", "Midboard", "Backboard"];

/** How classes are decided, for tooltips. Mirrors the constants in crates/mechabellum/src/replays/summary.rs. */
export const RULES: Tip = {
  title: "Deployment classes",
  lines: [
    "Aggressive: front edge within 3 squares of the line, or in a flank column.",
    "Midboard: from 5 squares in front of the towers back to their centers.",
    "Backboard: behind the towers' centers.",
    "Standard ground is the rest of the zone, between aggressive and midboard.",
    "Each unit counts by the ground it covers. An army is aggressive, midboard or backboard when that band holds the most of its ground and at least 40%; otherwise it's standard, even if little of it stands on standard ground.",
    "Aggressive and midboard armies are asym when three quarters or more of that band's ground stands on one side of the board, and sym otherwise. Standard and backboard aren't split.",
  ],
  note: "A match is classed by round 1's layout; later fights are classed the same way to show how the board grew. 1v1 on the 2.0 board, build 2259 on.",
};

/** The band of a deployment label: "Sym midboard" and "Asym midboard" are both midboard, and so
 * on. -1 when unknown. */
export function labelBand(label: string | null | undefined) {
  if (!label) return -1;
  const band = label.replace(/^(Sym|Asym) /, "").toLowerCase();
  return BANDS.findIndex((b) => b.toLowerCase() === band);
}

/** A band's color; unclassified marks get the muted ink (3:1 on the panel), not the near-invisible track. */
export const bandColor = (band: number) => (band >= 0 && band < 4 ? `var(--band-${band})` : "var(--text-muted)");

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
  "Fights won": "Fights won over the match, the last one included.",
  "Final army": "Units fielded in the last fight.",
  "Peak army": "Most units fielded in any one fight.",
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
  rows: [string, number | null | undefined, number | null | undefined, number][],
  note?: string,
) {
  heads[0].classList.add("tale-head", "left");
  heads[1].classList.add("tale-head", "right");
  const cells: HTMLElement[] = [heads[0], el("div"), heads[1]];
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
    const name = el("div", "tale-label", label);
    // One keyboard stop per row, so the explanation is reachable without a mouse.
    name.tabIndex = 0;
    const row = [
      el("div", `tale-value left${a > b ? " hi" : ""}`, a.toFixed(digits)),
      track(a, a > b, "left"),
      name,
      track(b, b > a, "right"),
      el("div", `tale-value${b > a ? " hi" : ""}`, b.toFixed(digits)),
    ];
    const tip: Tip = {
      title: label,
      lines: [MEASURES[label] ?? "", `${names[0]}: ${a.toFixed(digits)}`, `${names[1]}: ${b.toFixed(digits)}`],
      note,
    };
    // The whole row answers the hover, not just the label.
    for (const cell of row) cell.dataset.tip = JSON.stringify(tip);
    cells.push(...row);
  }
  container.replaceChildren(...cells);
}
