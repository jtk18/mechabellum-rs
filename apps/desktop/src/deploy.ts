// Deployment classes (mechabellum::classify) and whole-match summaries (mechabellum::PlayerGame).
import { el, pct, type Tip } from "./util";

export interface Deployment {
  /** "Standard", "Sym aggressive", "Asym aggressive", "Midboard" or "Backboard". */
  label: string;
  depth: "aggressive" | "standard" | "midboard" | "backboard";
  /** Aggressive only: -1 all on the player's own left, 1 all on their right. */
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
    "Aggressive: front edge within 3 squares of the line, or in a flank column. Sym when that group spans both sides, asym when three quarters of it is on one side.",
    "Midboard: from 5 squares in front of the towers back to their centers.",
    "Backboard: behind the towers' centers.",
    "Each unit counts by the ground it covers. An army is aggressive, midboard or backboard when that band holds the most of it and at least 40%; otherwise standard.",
  ],
  note: "A match is classed by round 1's layout; later fights are classed the same way to show how the board grew. 1v1 on the 2.0 board, build 2259 on.",
};

/** The band of a deployment label: both aggressive labels are the front band. -1 when unknown. */
export function labelBand(label: string | null | undefined) {
  if (!label) return -1;
  if (label.endsWith("ggressive")) return 0;
  return BANDS.indexOf(label);
}

export const bandColor = (band: number) => (band >= 0 && band < 4 ? `var(--band-${band})` : "var(--track)");

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
