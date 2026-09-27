# Board code

A short, forum-style way to write down a board: who stands where, at what level, with which
techs and items. You can paste it into a forum post or chat, copy it from any round of a
replay, and (later) have the tester set it up in Testing Grounds.

```text
[board version=2324 map="Railway Town" round=5]
[side name="jtk18" hp=4500]
[unit=Crawler at=-12.5,-20 level=2 techs="Acid Crawler, Flame Crawler"]
[unit=Fortress at=0,-25 rotated items="Heavy Armor"]
[/side]
[side name="Opponent"]
[unit=Marksman at=3,22]
[/side]
[/board]
```

## Tags

| Tag | Attributes | Notes |
|---|---|---|
| `[board]` … `[/board]` | `version`, `map`, `round` (all optional) | One per code. Text before and after it is ignored. |
| `[side]` … `[/side]` | `name`, `hp` (optional). `[side=Name]` is short for `name`. | Up to 8 sides. |
| `[unit=Name]` | `at` (required), `level`, `rotated`, `techs`, `items` | No closing tag. Up to 300 per side. |

- **Unit names** are the game's English names, matched without regard to case, spaces, or
  punctuation (`fire badger` finds Firebadger). A unit's number also works (`[unit=10 ...]`).
- **`at=x,y`** is the unit's center in grid squares from the middle of the board, the way
  replays store it. Halves are allowed (`-12.5`), since even-sized units sit between squares.
  Positive `y` is one army's half and negative `y` the other's.
- **`level`** is the level the game shows, from 1. It defaults to 1.
- **`rotated`** takes no value.
- **`techs`** and **`items`** are lists separated by commas or semicolons, up to 16 entries.
- **Values** with spaces go in double quotes. Quotes can't appear inside a value.

## Limits

Codes come from strangers, so the reader is strict and bounded:

- 64 KiB per code, 8 sides, 300 units per side, 16 techs or items per unit, 80 characters
  per name or value, levels 1 to 20, positions within 100 squares of the middle.
- Unknown tags, attributes, and units are errors, with the line and column of the problem.
- Text between tags isn't allowed, so a typo can't silently drop a unit.
- A code is only ever parsed into typed values; it never reaches SQL or HTML as text.

## Getting one

- **Desktop app:** open a match, pick a round, and choose "Copy board code".
- **Command line:** `replay-parse board <replay> [--round N]` prints the code for round `N`
  (the last round by default).
