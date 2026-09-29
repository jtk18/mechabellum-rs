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
| `command-center`, `research-center` | the towers' centers (see below); use an offset |

**Geometry** (the same on every 2.0 map measured, 82 games on 5 maps): each side's zone is 60
squares wide and 30 deep, and its front edge stops 1 square short of the middle, so a 2-square
strip separates the two boards. From round 3, flank columns 6 squares wide open along both edges
of the opponent's half, running its full depth. That's the "front moves up" change mid-game. A
flank anchor used with `round` below 3 is an error.

**Towers:** the Command Center stands at about (-14, -17.5) and the Research Center at about
(14, -17.5) in a side's own frame, on every 2.0 map. They were measured in Testing Grounds,
since replays don't record them. Give an offset (e.g. `from=command-center forward=5`): the unit
is centered at the offset, and the game nudges units off a tower's footprint (about a square
or two).

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
