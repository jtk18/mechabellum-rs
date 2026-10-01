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
| `[building=Name]` | `at` (required) | A wall, cannon, or barricade. No closing tag. Up to 50 per side. |
| `[device=Name]` | `at` (required) | Shield Generator, Sentry Missile, or Missile Interceptor. Up to 50 per side. |

- **Unit names** are the game's English names, matched without regard to case, spaces, or
  punctuation (`fire badger` finds Firebadger). A unit's number also works (`[unit=10 ...]`).
- **`at=x,y`** is the unit's center in grid squares from the middle of the board, the way
  replays store it. Halves are allowed (`-12.5`), since even-sized units sit between squares,
  and so are tenths (`23.4`), since devices can stand anywhere.
  Positive `y` is one army's half and negative `y` the other's.
- **`level`** is the level the game shows, from 1. It defaults to 1.
- **`rotated`** takes no value.
- **`techs`** and **`items`** are lists separated by commas or semicolons, up to 16 entries.
- **Items** go by name. Where several items share a name (Heavy Armor comes in two ranks, and
  rank 1 alone has two Haste Modules), add the ID: `items="Heavy Armor #13030013"`. The name must
  match the ID; `#13030013` alone also works. Codes copied from replays add the ID only where
  it's needed.
- **Values** with spaces go in double quotes. Quotes can't appear inside a value.

## Anchors: positions relative to landmarks

Instead of `at=x,y`, a unit can be placed relative to a landmark on its own side's half:

```text
[board round=4]
[side name="jtk18" half=south]
[unit=Fortress from=front]
[unit=Crawler from=front-left right=2 back=1]
[unit=Marksman from=back-right left=3.5]
[unit=Fang from=right-flank forward=4]
[/side]
[/board]
```

- **`half=south` or `half=north`** on the side says which half it deploys on (south is negative
  `y` in replays). Anchors need it; codes copied from replays include it.
- **Directions are the side's own view:** `right`/`left` across the board, `forward` toward the
  middle, `back` toward its own edge. Values are squares, and halves are allowed. The same
  code works for either half.
- **Anchors** (the unit lines up flush against the anchor's edges, so a corner anchor puts it
  in the corner, not centered on it):

| Anchor | Where |
|---|---|
| `center` | the middle of the board (the unit's center sits on it) |
| `front`, `back` | the middle of the deployment zone's front or back edge |
| `left`, `right` | the middle of the zone's left or right edge |
| `front-left`, `front-right`, `back-left`, `back-right` | the zone's corners |
| `left-flank`, `right-flank` | the near end of a flank column, just past the middle; from round 3 |
| `command-center` (`cc`), `research-center` (`rc`) | a tower's center; add a face to stand beside it (below) |
| a name given with `id=` | that unit (or row) on the same side; add a face to stand beside it |

**Faces:** a tower or a named unit followed by `-front`, `-back`, `-left`, `-right`, or a corner
(`-front-left`, `-back-right`, ...) puts the new unit flush against that side of it, outside it,
centered along it (or diagonally off the corner). Offsets still apply.

```text
[side half=south]
[unit=Arclight from=rc-front]                  touching the Research Center's front
[unit=Tarantula from=research-center-back]     touching its back
[unit=Fortress id=wall from=front]             a name for later units
[unit=Marksman from=wall-back left=1]          behind the Fortress, a square left
[unit=Crawler count=5 gap=1 id=row from=front-left]
[unit=Fang from=row-back]                      centered behind the whole row
[/side]
```

**Names:** `id=wall` names a unit (a letter, then letters, digits, or `_`; no `-`, since that
starts a face). A name works for the units after it on the same side, and can't reuse an
anchor's name.

**Rows:** `count=N` (up to 60) makes N copies, each next one flush beside the last, `gap=`
squares apart (default 0), toward the side's right unless `toward=left`, `forward`, or `back`.
An `id=` on a row names the whole row's outline.

**Geometry** (the same on every 2.0 map measured, 82 games on 5 maps): each side's zone is 60
squares wide and 30 deep, and its front edge stops 1 square short of the middle, so a 2-square
strip separates the two boards. From round 3, flank columns 6 squares wide (x 30 to 36) open
along both edges. That's the "front moves up" change mid-game. A flank anchor used with `round`
below 3 is an error (though cards can allow it: 36 of 3,000 flank units in 2.0 replays stood
there in rounds 1 and 2).

**Towers:** each tower covers exactly 2×2 squares: the Research Center x 13 to 15, y -18 to -16,
centered at (14, -17), and the Command Center mirrored at (-14, -17), in a side's own frame, on
every 2.0 map. No unit covers those squares in 2,821 replay boards, and every square around
them is covered hundreds of times. Units can stand flush against every side.

## Checking a code

`replay-parse check-board FILE` reads every `[board]` in the file and reports what the game
wouldn't allow: units overlapping each other or a tower, poking out of the zone, or outside the
zone and the flank columns. Unit sizes come from `footprint` in `game_data.rs`; checking the
2,821 replay boards corrected five of them (2026-09-30), and 2,803 of the boards now pass. The
rest have an Abyss or a few small units overlapping by a square, or units ahead of the zone
(cards that deploy forward).

## Buildings

`[building="Defensive Wall" at=-14,-5.5]` is a building the side has put up: Defensive Wall,
Anti-Armor Cannon, Rapid-Fire Cannon, or Magnetic Barricade. Several game IDs share a name, so
the variant's ID can stand in for the name (`[building=5 at=0,-3]`). Codes copied from replays
use the ID whenever the name alone would pick a different variant.

In every replay checked (60 games, 2.0), buildings stand only in fixed sockets, lined up with the
towers: x = ±14, with walls at y = ∓5.5 and cannons at y = ∓10 (the south half's signs).
Replays record buildings as they stood for each fight; for the last round, only those standing
as its deployment began.

## Devices

`[device="Sentry Missile" at=23.4,-27.2]` is a device the side has set up: Shield Generator,
Sentry Missile, or Missile Interceptor (19 of 50 recent games had at least one). Devices stand
anywhere on the half, not in sockets. A Missile Interceptor is used up when it stops a missile,
so the replay's record after the fight loses it; codes copied from a replay add the devices that
stood as the round began to make up for that, but one set up and used up in the same round
can't be seen.

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
