// One match, redrawn round by round on a schematic board.
import { invoke } from "@tauri-apps/api/core";
import { $, el, modeLabel, type Tip } from "./util";

interface TechRef {
  id: string;
  name: string | null;
}

interface UnitTechs {
  unit_id: number;
  unit_name: string;
  techs: TechRef[];
}

interface PlacedUnit {
  unit_id: number;
  name: string;
  index: number;
  level: number;
  exp: number;
  xp_to_next: number | null;
  x: number;
  y: number;
  rotated: boolean;
  footprint: [number, number] | null;
  equipment: string[];
}

interface SideLayout {
  result: string;
  hp: number;
  units: PlacedUnit[];
  enabled_techs: UnitTechs[];
  specialists: string[];
}

interface MatchDetail {
  battle_id: string;
  version: number;
  map_id: string;
  map_name: string | null;
  match_mode: string;
  game_mode: string;
  players: { id: string; name: string; team: string; won: boolean | null; loadout: UnitTechs[] }[];
  rounds: { round: number; sides: (SideLayout | null)[] }[];
}

const SVG = "http://www.w3.org/2000/svg";
/** Replay units per grid square. */
const SQUARE = 10;
/** Footprint used when a unit's size is unknown. */
const FALLBACK: [number, number] = [2, 2];

let detail: MatchDetail | null = null;
/** Player index drawn at the bottom of the board. */
let bottom = 0;
let roundIndex = 0;
let selected: { side: number; index: number } | null = null;
/** Maps game coordinates to board coordinates, fixed per match so rounds don't jump around. */
let toBoard: (x: number, y: number) => [number, number] = (x, y) => [x, -y];

function svg<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>) {
  const node = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

function size(unit: PlacedUnit): [number, number] {
  const [w, d] = unit.footprint ?? FALLBACK;
  return unit.rotated ? [d * SQUARE, w * SQUARE] : [w * SQUARE, d * SQUARE];
}

function sideClass(side: number) {
  return side === bottom ? "side-near" : "side-far";
}

/** Short on-board label, e.g. "Mus" for Mustang. */
function abbreviation(name: string) {
  const words = name.split(/\s+/);
  return words.length > 1 ? words.map((w) => w[0]).join("").slice(0, 3) : name.slice(0, 3);
}

function setupFrame(m: MatchDetail) {
  const all = m.rounds.flatMap((r) => r.sides.flatMap((s, i) => (s ? s.units.map((u) => ({ u, i })) : [])));
  // The player at the bottom sees the board as they did in game: rotate 180° when their army
  // deployed on the positive-y half.
  const nearY = all.filter((a) => a.i === bottom).map((a) => a.u.y);
  const nearIsPositive = nearY.length > 0 && nearY.reduce((a, b) => a + b, 0) / nearY.length > 0;
  toBoard = nearIsPositive ? (x, y) => [-x, y] : (x, y) => [x, -y];
}

function drawBoard() {
  const m = detail!;
  const board = $<HTMLElement>("board") as unknown as SVGSVGElement;
  const sides = m.rounds[roundIndex].sides;

  // Fit the view to every unit in the match, plus a margin, snapped to the grid.
  const boxes = m.rounds.flatMap((r) =>
    r.sides.flatMap((s) =>
      (s?.units ?? []).map((u) => {
        const [bx, by] = toBoard(u.x, u.y);
        const [w, h] = size(u);
        return [bx - w / 2, by - h / 2, bx + w / 2, by + h / 2];
      }),
    ),
  );
  const pad = 3 * SQUARE;
  const snap = (v: number, up: boolean) => (up ? Math.ceil(v / SQUARE) : Math.floor(v / SQUARE)) * SQUARE;
  const minX = snap(Math.min(...boxes.map((b) => b[0])) - pad, false);
  const minY = snap(Math.min(...boxes.map((b) => b[1])) - pad, false);
  const maxX = snap(Math.max(...boxes.map((b) => b[2])) + pad, true);
  const maxY = snap(Math.max(...boxes.map((b) => b[3])) + pad, true);
  board.setAttribute("viewBox", `${minX} ${minY} ${maxX - minX} ${maxY - minY}`);

  // Midline between the two deployment areas.
  const nearTop = Math.min(
    ...m.rounds.flatMap((r) => (r.sides[bottom]?.units ?? []).map((u) => toBoard(u.x, u.y)[1] - size(u)[1] / 2)),
  );
  const farBottom = Math.max(
    ...m.rounds.flatMap((r) =>
      r.sides.flatMap((s, i) => (i === bottom || !s ? [] : s.units.map((u) => toBoard(u.x, u.y)[1] + size(u)[1] / 2))),
    ),
  );
  const mid = Number.isFinite(nearTop) && Number.isFinite(farBottom) ? (nearTop + farBottom) / 2 : 0;

  const layers: SVGElement[] = [
    svg("rect", { class: "zone side-far", x: minX, y: minY, width: maxX - minX, height: Math.max(0, mid - minY) }),
    svg("rect", { class: "zone side-near", x: minX, y: mid, width: maxX - minX, height: Math.max(0, maxY - mid) }),
  ];
  const grid = svg("g", { class: "grid" });
  for (let x = minX; x <= maxX; x += SQUARE) {
    grid.append(svg("line", { x1: x, y1: minY, x2: x, y2: maxY, class: x % 50 === 0 ? "major" : "" }));
  }
  for (let y = minY; y <= maxY; y += SQUARE) {
    grid.append(svg("line", { x1: minX, y1: y, x2: maxX, y2: y, class: y % 50 === 0 ? "major" : "" }));
  }
  layers.push(grid, svg("line", { class: "midline", x1: minX, y1: mid, x2: maxX, y2: mid }));

  const units = svg("g", { class: "units" });
  sides.forEach((side, s) => {
    for (const u of side?.units ?? []) {
      const [bx, by] = toBoard(u.x, u.y);
      const [w, h] = size(u);
      const isSelected = selected?.side === s && selected.index === u.index;
      const g = svg("g", {
        class: `unit ${sideClass(s)}${isSelected ? " selected" : ""}`,
        tabindex: 0,
        role: "button",
        "aria-label": `${u.name}, level ${u.level + 1}, ${detail!.players[s].name}`,
      });
      g.append(svg("rect", { x: bx - w / 2 + 0.6, y: by - h / 2 + 0.6, width: w - 1.2, height: h - 1.2, rx: 1.6 }));
      if (w >= 20 && h >= 16) {
        const label = svg("text", { x: bx, y: by + 2.2, "text-anchor": "middle" });
        label.textContent = abbreviation(u.name);
        g.append(label);
      }
      // One mark per level gained, top-left, up to five.
      for (let l = 0; l < Math.min(u.level, 5); l++) {
        g.append(svg("rect", { class: "pip", x: bx - w / 2 + 2 + l * 2.6, y: by - h / 2 + 2, width: 1.8, height: 1.8 }));
      }
      const tip: Tip = { title: u.name, lines: [`Level ${u.level + 1}, ${detail!.players[s].name}`] };
      g.dataset.tip = JSON.stringify(tip);
      const choose = () => {
        selected = { side: s, index: u.index };
        render();
      };
      g.addEventListener("click", choose);
      g.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          choose();
        }
      });
      units.append(g);
    }
  });
  layers.push(units);
  board.replaceChildren(...layers);
}

function techList(loadout: TechRef[], enabled: TechRef[]) {
  const enabledIds = new Set(enabled.map((t) => t.id));
  const extra = enabled.filter((t) => !loadout.some((l) => l.id === t.id));
  const list = el("ul", "tech-list");
  for (const tech of [...loadout, ...extra]) {
    const on = enabledIds.has(tech.id);
    const item = el("li", on ? "on" : "off");
    item.append(el("span", "tech-mark", on ? "●" : "○"), el("span", "", tech.name ?? `Tech ${tech.id}`));
    item.append(el("span", "tech-state", on ? "researched" : "not researched"));
    list.append(item);
  }
  if (list.children.length === 0) list.append(el("li", "off", "No techs recorded."));
  return list;
}

function renderUnitPanel() {
  const m = detail!;
  const panel = $("unit-panel");
  const sides = m.rounds[roundIndex].sides;
  const unit = selected ? sides[selected.side]?.units.find((u) => u.index === selected!.index) : undefined;

  if (!selected || !unit) {
    // No unit chosen: summarize both sides for this round.
    const blocks = sides.map((side, s) => {
      const block = el("section", `side-summary ${sideClass(s)}`);
      block.append(el("h3", "", m.players[s].name));
      if (!side) {
        block.append(el("p", "muted", "No army recorded this round."));
        return block;
      }
      block.append(
        el("p", "", `${side.result === "Win" ? "Won" : "Lost"} the round with ${side.hp.toLocaleString()} HP left`),
        el("p", "muted", `${side.units.length} units${side.specialists.length ? `, specialists: ${side.specialists.join(", ")}` : ""}`),
      );
      const researched = side.enabled_techs.filter((u) => u.techs.length);
      if (researched.length) {
        const list = el("ul", "tech-summary");
        for (const u of researched) {
          const item = el("li");
          item.append(el("strong", "", u.unit_name), document.createTextNode(`: ${u.techs.map((t) => t.name ?? t.id).join(", ")}`));
          list.append(item);
        }
        block.append(el("h4", "", "Researched techs"), list);
      }
      return block;
    });
    panel.replaceChildren(el("p", "hint", "Click a unit on the board to see its level, XP, and techs."), ...blocks);
    return;
  }

  const s = selected.side;
  const player = m.players[s];
  const card = el("section", `unit-card ${sideClass(s)}`);
  card.append(el("h3", "", unit.name), el("p", "muted", `${player.name}, unit #${unit.index + 1}`));

  const level = el("div", "level");
  level.append(el("span", "level-number", `Level ${unit.level + 1}`));
  if (unit.xp_to_next) {
    const bar = el("div", "xp-bar");
    const fill = el("div", "xp-fill");
    fill.style.width = `${Math.min(1, unit.exp / unit.xp_to_next) * 100}%`;
    bar.append(fill);
    level.append(bar, el("span", "xp-text", `${unit.exp.toLocaleString()} / ${unit.xp_to_next.toLocaleString()} XP to level ${unit.level + 2}`));
  } else {
    level.append(el("span", "xp-text", `${unit.exp.toLocaleString()} XP (next level's requirement not known yet)`));
  }
  card.append(level);

  if (unit.equipment.length) {
    card.append(el("h4", "", "Equipment"), el("p", "", unit.equipment.join(", ")));
  }
  const loadout = player.loadout.find((l) => l.unit_id === unit.unit_id)?.techs ?? [];
  const enabled = sides[s]?.enabled_techs.find((l) => l.unit_id === unit.unit_id)?.techs ?? [];
  card.append(el("h4", "", "Techs"), techList(loadout, enabled));

  const clear = el("button", "quiet", "Show both sides") as HTMLButtonElement;
  clear.type = "button";
  clear.addEventListener("click", () => {
    selected = null;
    render();
  });
  card.append(clear);
  panel.replaceChildren(card);
}

function renderRoundBar() {
  const m = detail!;
  const round = m.rounds[roundIndex];
  $("round-label").textContent = `Round ${round.round} of ${m.rounds[m.rounds.length - 1].round}`;
  const input = $<HTMLInputElement>("round-input");
  input.max = String(m.rounds.length - 1);
  input.value = String(roundIndex);
  $<HTMLButtonElement>("round-prev").disabled = roundIndex === 0;
  $<HTMLButtonElement>("round-next").disabled = roundIndex === m.rounds.length - 1;
  $("round-results").replaceChildren(
    ...round.sides.map((side, s) => {
      const chip = el("span", `result-chip ${sideClass(s)}`);
      chip.append(el("span", "swatch"), document.createTextNode(`${m.players[s].name}: ${side ? `${side.result === "Win" ? "won" : "lost"}, ${side.hp.toLocaleString()} HP` : "no record"}`));
      return chip;
    }),
  );
}

function render() {
  renderRoundBar();
  drawBoard();
  renderUnitPanel();
}

function step(delta: number) {
  if (!detail) return;
  roundIndex = Math.max(0, Math.min(detail.rounds.length - 1, roundIndex + delta));
  // Keep the same unit selected across rounds when it still exists.
  render();
}

let wired = false;
function wire(onBack: () => void) {
  if (wired) return;
  wired = true;
  $("match-back").addEventListener("click", onBack);
  $("round-prev").addEventListener("click", () => step(-1));
  $("round-next").addEventListener("click", () => step(1));
  $<HTMLInputElement>("round-input").addEventListener("input", (e) => {
    roundIndex = Number((e.target as HTMLInputElement).value);
    render();
  });
  document.addEventListener("keydown", (e) => {
    if ($("view-match").hidden || (e.target as HTMLElement).tagName === "INPUT") return;
    if (e.key === "ArrowLeft") step(-1);
    if (e.key === "ArrowRight") step(1);
    if (e.key === "Escape") onBack();
  });
}

/** Show one match. `mainPlayerId` picks whose side is drawn at the bottom. */
export async function openMatch(battleId: string, mainPlayerId: string | null, onBack: () => void) {
  wire(onBack);
  detail = await invoke<MatchDetail>("match_detail", { battleId });
  const m = detail;
  bottom = Math.max(0, m.players.findIndex((p) => p.id === mainPlayerId));
  roundIndex = Math.max(0, m.rounds.length - 1);
  selected = null;
  setupFrame(m);

  const [near, ...others] = [m.players[bottom], ...m.players.filter((_, i) => i !== bottom)];
  $("match-title").textContent = `${near.name} vs ${others.map((p) => p.name).join(", ")}`;
  const outcome = near.won === null ? "" : near.won ? `${near.name} won. ` : `${near.name} lost. `;
  $("match-meta").textContent = `${outcome}${m.map_name ?? `Map ${m.map_id}`}, ${modeLabel(m.match_mode, m.game_mode)}, version ${m.version}`;

  $("view-stats").hidden = true;
  $("view-match").hidden = false;
  window.scrollTo(0, 0);

  if (m.rounds.length === 0) {
    $("round-results").replaceChildren();
    ($("board") as unknown as SVGSVGElement).replaceChildren();
    $("unit-panel").replaceChildren(el("p", "hint", "This replay has no recorded fights. The match ended before round 1 finished."));
    return;
  }
  render();
}
