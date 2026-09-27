// One match, redrawn round by round on a schematic board.
import { invoke } from "@tauri-apps/api/core";
import { art, artImage } from "./art";
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
  equipment: { id: string; name: string | null }[];
}

interface CardRef {
  id: string;
  name: string | null;
  kind: "specialist" | "unit" | "skill" | "equipment" | "other";
  unit_id: number | null;
}

interface Structure {
  level: number | null;
  items: { id: string; name: string | null }[];
}

interface SideLayout {
  result: string;
  hp: number;
  units: PlacedUnit[];
  enabled_techs: UnitTechs[];
  specialists: string[];
  cards_offered: CardRef[];
  cards_chosen: CardRef[];
  command_center: Structure;
  research_center: Structure;
  /** Units rebuilt from the round's actions (the last round has no snapshot after it). */
  rebuilt: boolean;
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

  // Midline between the two armies in this round. Deployment areas shift between rounds and
  // maps, so it is measured per round rather than fixed.
  const nearTop = Math.min(
    ...(sides[bottom]?.units ?? []).map((u) => toBoard(u.x, u.y)[1] - size(u)[1] / 2),
  );
  const farBottom = Math.max(
    ...sides.flatMap((s, i) => (i === bottom || !s ? [] : s.units.map((u) => toBoard(u.x, u.y)[1] + size(u)[1] / 2))),
  );
  // When the armies overlap (forward flank deployments, units deep on the other side) no
  // straight line separates them, so draw no halves rather than misleading ones.
  const separated = Number.isFinite(nearTop) && Number.isFinite(farBottom) && farBottom <= nearTop;
  const mid = (nearTop + farBottom) / 2;

  const layers: SVGElement[] = [
    svg("rect", { class: "surface", x: minX, y: minY, width: maxX - minX, height: maxY - minY }),
  ];
  if (separated) {
    layers.push(
      svg("rect", { class: "zone side-far", x: minX, y: minY, width: maxX - minX, height: Math.max(0, mid - minY) }),
      svg("rect", { class: "zone side-near", x: minX, y: mid, width: maxX - minX, height: Math.max(0, maxY - mid) }),
    );
  }
  const grid = svg("g", { class: "grid" });
  for (let x = minX; x <= maxX; x += SQUARE) {
    grid.append(svg("line", { x1: x, y1: minY, x2: x, y2: maxY, class: x % 50 === 0 ? "major" : "" }));
  }
  for (let y = minY; y <= maxY; y += SQUARE) {
    grid.append(svg("line", { x1: minX, y1: y, x2: maxX, y2: y, class: y % 50 === 0 ? "major" : "" }));
  }
  layers.push(grid);
  if (separated) layers.push(svg("line", { class: "midline", x1: minX, y1: mid, x2: maxX, y2: mid }));

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
      const box = { x: bx - w / 2 + 0.6, y: by - h / 2 + 0.6, width: w - 1.2, height: h - 1.2 };
      g.append(svg("rect", { ...box, rx: 1.6 }));
      const icon = art.unitIcon(u.unit_id);
      if (icon) {
        // A nested <svg> clips the art to the footprint.
        const frame = svg("svg", { ...box, viewBox: "0 0 100 100", preserveAspectRatio: "xMidYMid slice", class: "art" });
        frame.append(svg("image", { href: icon, width: 100, height: 100, preserveAspectRatio: "xMidYMid slice" }));
        g.append(frame, svg("rect", { ...box, rx: 1.6, class: "tint" }), svg("rect", { ...box, rx: 1.6, class: "edge" }));
      } else if (w >= 20 && h >= 16) {
        const label = svg("text", { x: bx, y: by + 2.2, "text-anchor": "middle" });
        label.textContent = abbreviation(u.name);
        g.append(label);
      }
      // One mark per level gained, top-left, up to five.
      for (let l = 0; l < Math.min(u.level, 5); l++) {
        g.append(svg("rect", { class: "pip", x: bx - w / 2 + 2 + l * 2.6, y: by - h / 2 + 2, width: 1.8, height: 1.8 }));
      }
      const tip: Tip = { title: u.name, lines: [`Level ${u.level + 1}`, `Owned by ${detail!.players[s].name}`] };
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
    const mark = artImage(art.techIcon(tech.id), "tech-icon") ?? el("span", "tech-mark", on ? "●" : "○");
    item.append(mark, el("span", "", tech.name ?? `Tech ${tech.id}`));
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
      block.append(structureCard("Command Center", art.commandCenter(), side.command_center, "No skills active"));
      block.append(structureCard("Research Center", art.researchCenter(), side.research_center, "No blueprints"));
      if (side.cards_offered.length) {
        block.append(el("h4", "", "Cards offered this round"), cardRow(side.cards_offered, side.cards_chosen));
      } else if (side.cards_chosen.length) {
        block.append(el("h4", "", "Cards picked this round"), cardRow(side.cards_chosen, side.cards_chosen));
      }
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
  const portrait = artImage(art.unitPortrait(unit.unit_id), "portrait", unit.name);
  if (portrait) card.append(portrait);
  card.append(el("h3", "", unit.name), el("p", "muted", `Owned by ${player.name}, unit #${unit.index + 1}`));

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
    const list = el("ul", "equipment-list");
    for (const item of unit.equipment) {
      const entry = el("li");
      const icon = artImage(art.equipment(item.id), "equipment-icon");
      if (icon) entry.append(icon);
      entry.append(el("span", "", item.name ?? `#${item.id}`));
      list.append(entry);
    }
    card.append(el("h4", "", "Equipment"), list);
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

function structureCard(title: string, image: string | null, structure: Structure, empty: string) {
  const card = el("div", "structure");
  const picture = artImage(image, "structure-art", title);
  if (picture) card.append(picture);
  const text = el("div", "structure-text");
  const heading = el("strong", "", title);
  text.append(heading);
  if (structure.level !== null) text.append(el("span", "muted", ` level ${structure.level + 1}`));
  const items = structure.items.map((i) => i.name ?? `#${i.id}`);
  text.append(el("p", items.length ? "" : "muted", items.length ? items.join(", ") : empty));
  card.append(text);
  return card;
}

const CARD_KIND: Record<CardRef["kind"], string> = {
  specialist: "Specialist",
  unit: "Unit",
  skill: "Skill",
  equipment: "Item",
  other: "Card",
};

function cardRow(cards: CardRef[], chosen: CardRef[]) {
  const picked = new Set(chosen.map((c) => c.id));
  const row = el("ul", "card-row");
  for (const card of cards) {
    const isPicked = picked.has(card.id);
    const item = el("li", `card ${card.kind}${isPicked ? " picked" : ""}`);
    const image =
      card.kind === "specialist"
        ? art.specialist(card.id) ?? (card.unit_id !== null ? art.unitIcon(card.unit_id) : null)
        : card.kind === "equipment"
          ? art.equipment(card.id)
          : card.unit_id !== null
          ? art.unitIcon(card.unit_id)
          : null;
    item.append(artImage(image, "card-art") ?? el("span", "card-art placeholder"));
    item.append(el("span", "card-name", card.name ?? `#${card.id}`), el("span", "card-kind", isPicked ? "Picked" : CARD_KIND[card.kind]));
    row.append(item);
  }
  return row;
}

function renderRoundBar() {
  const m = detail!;
  const round = m.rounds[roundIndex];
  // Count positions rather than trusting round numbers: some modes start at 0.
  $("round-label").textContent = `Round ${roundIndex + 1} of ${m.rounds.length}`;
  const input = $<HTMLInputElement>("round-input");
  input.max = String(m.rounds.length - 1);
  input.value = String(roundIndex);
  $<HTMLButtonElement>("round-prev").disabled = roundIndex === 0;
  $<HTMLButtonElement>("round-next").disabled = roundIndex === m.rounds.length - 1;
  $("round-results").replaceChildren(
    ...round.sides.map((side, s) => {
      const chip = el("span", `result-chip ${sideClass(s)}`);
      const result = side?.result === "Win" ? "won" : side?.result === "Deuce" ? "drew" : "lost";
      chip.append(el("span", "swatch"), document.createTextNode(`${m.players[s].name}: ${side ? `${result}, ${side.hp.toLocaleString()} HP going in` : "no record"}`));
      return chip;
    }),
  );
  $("board-note-rebuilt").hidden = !round.sides.some((side) => side?.rebuilt);
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
  const copy = $<HTMLButtonElement>("copy-board");
  copy.addEventListener("click", async () => {
    if (!detail) return;
    try {
      const code = await invoke<string>("board_code", { battleId: detail.battle_id, roundIndex });
      await invoke("copy_text", { text: code });
      copy.textContent = "Copied";
    } catch (e) {
      copy.textContent = "Couldn't copy";
      copy.title = String(e);
    }
    window.setTimeout(() => (copy.textContent = "Copy board code"), 2000);
  });
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

/** Redraw the open match, e.g. once game art has been extracted. */
export function redrawMatch() {
  if (detail && detail.rounds.length > 0 && !$("view-match").hidden) render();
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
  const ratings = await invoke<{ player_id: string; elo: number | null }[]>("match_ratings", { battleId }).catch(() => []);
  const titled = (p: { id: string; name: string }) => {
    const elo = ratings.find((r) => r.player_id === p.id)?.elo;
    return elo == null ? p.name : `${p.name} (${elo} MMR)`;
  };
  $("match-title").textContent = `${titled(near)} vs ${others.map(titled).join(", ")}`;
  const outcome = near.won === null ? "" : near.won ? `${near.name} won. ` : `${near.name} lost. `;
  $("match-meta").textContent = `${outcome}${m.map_name ?? `Map ${m.map_id}`}, ${modeLabel(m.match_mode, m.game_mode)}, version ${m.version}`;


  if (m.rounds.length === 0) {
    $("round-results").replaceChildren();
    ($("board") as unknown as SVGSVGElement).replaceChildren();
    $("unit-panel").replaceChildren(el("p", "hint", "This replay has no recorded fights. The match ended before round 1 finished."));
    return;
  }
  render();
}
