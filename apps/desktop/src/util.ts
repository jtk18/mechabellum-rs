export const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export const pct = (x: number) => `${Math.round(x * 100)}%`;

/** A name set apart from the text around it: an uploaded name with direction controls (U+202E)
 * otherwise turns the rest of the sentence around. */
export const iso = (name: string) => `\u2068${name}\u2069`;

export function el(tag: string, className?: string, text?: string) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const MATCH_MODES: Record<string, string> = {
  VS_1_1: "1v1",
  VS_2_2: "2v2",
  VS_4_Scuffle: "4-player FFA",
};

export const modeLabel = (matchMode: string, gameMode: string) =>
  `${MATCH_MODES[matchMode] ?? matchMode} ${gameMode.replace(/([a-z])([A-Z])/g, "$1 $2")}`;

/** Tooltip content attached to a hoverable element as JSON in data-tip. */
export interface Tip {
  title: string;
  lines: string[];
  note?: string;
}
