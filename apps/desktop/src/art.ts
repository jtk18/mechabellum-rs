// Game art extracted locally from the player's install (see src-tauri/src/art.rs).
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ArtManifest {
  units: Record<string, { icon: string | null; portrait: string | null }>;
  techs: Record<string, string>;
  specialists: Record<string, string>;
  equipment?: Record<string, string>;
  specialist_fallback: string | null;
  command_center: string | null;
  research_center: string | null;
  background: string | null;
}

interface ArtInfo {
  dir: string;
  manifest: ArtManifest | null;
  status: string | null;
  units_by_name: Record<string, string>;
  techs_by_name: Record<string, string>;
  specialists_by_name: Record<string, string>;
}

let info: ArtInfo | null = null;

function url(file: string | null | undefined) {
  if (!file || !info) return null;
  const sep = info.dir.includes("\\") ? "\\" : "/";
  return convertFileSrc(`${info.dir}${sep}${file}`);
}

const m = () => info?.manifest ?? null;

/** Names differ in spacing and case between sources ("Firebadger" vs "Fire Badger"). */
const key = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, "");
const byKey = { units: new Map<string, string>(), techs: new Map<string, string>(), specialists: new Map<string, string>() };

function index() {
  for (const [kind, source] of [
    ["units", info?.units_by_name],
    ["techs", info?.techs_by_name],
    ["specialists", info?.specialists_by_name],
  ] as const) {
    byKey[kind].clear();
    for (const [name, file] of Object.entries(source ?? {})) byKey[kind].set(key(name), file);
  }
}

export const art = {
  status: () => info?.status ?? null,
  ready: () => m() !== null,
  unitIcon: (unitId: number) => url(m()?.units[unitId]?.icon),
  unitPortrait: (unitId: number) => url(m()?.units[unitId]?.portrait),
  techIcon: (techId: string | number) => url(m()?.techs[String(techId)]),
  specialist: (officerId: string | number) => url(m()?.specialists[String(officerId)]),
  equipment: (equipmentId: string | number) => url(m()?.equipment?.[String(equipmentId)]),
  commandCenter: () => url(m()?.command_center),
  researchCenter: () => url(m()?.research_center),
  background: () => url(m()?.background),
  unitIconByName: (name: string) => url(byKey.units.get(key(name))),
  techIconByName: (name: string) => url(byKey.techs.get(key(name))),
  specialistByName: (name: string) => url(byKey.specialists.get(key(name))),
};

/** Load the manifest, and call `onChange` whenever extraction finishes or reports progress. */
export async function loadArt(onChange: () => void) {
  info = await invoke<ArtInfo>("art_info");
  index();
  await listen<string>("art-status", async () => {
    info = await invoke<ArtInfo>("art_info");
    index();
    onChange();
  });
  onChange();
}

export function extractArtAgain() {
  return invoke("extract_art");
}

/** An <img> for a piece of art, or null when there is none. */
export function artImage(src: string | null, className: string, alt = "") {
  if (!src) return null;
  const img = document.createElement("img");
  img.className = className;
  img.src = src;
  img.alt = alt;
  img.loading = "lazy";
  img.decoding = "async";
  return img;
}
