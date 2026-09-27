import { invoke } from "@tauri-apps/api/core";
import { art, artImage } from "./art";
import { $, el, pct, type Tip } from "./util";

interface UnitLoadout {
  unit_id: number;
  unit_name: string | null;
  techs: { id: number; name: string | null }[];
}

interface TechStat {
  tech: string;
  tech_id: string | null;
  picks: number;
  wins: number;
}

interface UnitTechStats {
  unit: string;
  picks: number;
  wins: number;
  techs: TechStat[];
}

/** Units, each with the techs unlocked in Unit Modification and the player's results with them. */
export async function renderLoadout(me: string | null) {
  const grid = $("loadout-grid");
  const note = $("loadout-note");
  const [units, stats] = await Promise.all([
    invoke<UnitLoadout[]>("loadout"),
    me ? invoke<UnitTechStats[]>("tech_stats", { filter: { player_id: me } }) : Promise.resolve([]),
  ]);
  // Star Expedition copies (5000+) repeat the regular units.
  const shown = units.filter((u) => u.unit_id < 5000 && u.unit_name && u.techs.length > 0);
  if (shown.length === 0) {
    note.textContent = "Couldn't read your unit modifications. They're in GameData.grdb next to the game's Replay folder.";
    grid.replaceChildren();
    return;
  }
  note.textContent = "Read from the game's GameData.grdb. Results count your games where the unit was in your final army with that tech researched.";
  const byUnit = new Map(stats.map((s) => [s.unit, s]));
  const played = (u: UnitLoadout) => byUnit.get(u.unit_name!)?.picks ?? 0;
  shown.sort((a, b) => played(b) - played(a) || a.unit_name!.localeCompare(b.unit_name!));

  grid.replaceChildren(
    ...shown.map((u) => {
      const card = el("article", "loadout-card");
      const head = el("header", "loadout-head");
      const icon = artImage(art.unitIcon(u.unit_id), "loadout-unit-art", u.unit_name!);
      if (icon) head.append(icon);
      const unitStats = byUnit.get(u.unit_name!);
      const title = el("div", "loadout-title");
      title.append(el("strong", "", u.unit_name!));
      title.append(
        el(
          "span",
          "muted",
          unitStats ? `${unitStats.picks} games, ${pct(unitStats.wins / unitStats.picks)} won` : "No games yet",
        ),
      );
      head.append(title);
      card.append(head);

      const list = el("ul", "loadout-techs");
      for (const t of u.techs) {
        const item = el("li");
        const techIcon = artImage(art.techIcon(t.id), "loadout-tech-icon");
        item.append(techIcon ?? el("span", "loadout-tech-icon placeholder"));
        item.append(el("span", "loadout-tech-name", t.name ?? `#${t.id}`));
        const s = unitStats?.techs.find((x) => x.tech_id === String(t.id) || x.tech === t.name);
        if (s && s.picks > 0) {
          item.append(el("span", "loadout-tech-stat", `${s.picks} · ${pct(s.wins / s.picks)}`));
          const tip: Tip = {
            title: `${u.unit_name}: ${t.name ?? t.id}`,
            lines: [`Researched in ${s.picks} of your games`, `Won ${s.wins} of ${s.picks} (${pct(s.wins / s.picks)})`],
          };
          item.dataset.tip = JSON.stringify(tip);
        } else {
          item.append(el("span", "loadout-tech-stat muted", "unused"));
        }
        list.append(item);
      }
      card.append(list);
      return card;
    }),
  );
}
