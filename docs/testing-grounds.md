# Testing Grounds automation: what the game offers

Notes from a first look (2026-09-27, game 2.0.0.1.2324, 2560×1600 fullscreen). The goal is to
take a board code (docs/board-code.md) and set it up in Testing Grounds.

## Getting there

1. Main menu: **Standard Versus**. This opens the multiplayer lobby. Its **Play** button starts
   matchmaking, so never click it.
2. Top tabs: **Solo**, then the **Testing Grounds** card (or the sub-tab of the same name).
3. **Testing Ground Presets** panel. Fields for Me and Enemy: current HP, first round's
   supplies, supplies per round, max supplies; also deployment and combat timers, max rounds,
   "Select Initial Units", "Select Reinforcement", and **Map** (a dropdown).
4. **Enter the Testing Grounds**.

The presets cover a board code's `hp` per side and `map`.

## In the sandbox

- Top: HP bars, **Start** (runs the fight), and **Switch Sides**. Placing the enemy's units
  means switching sides, placing, and switching back.
- **Testing Grounds Tools** (button on the right) opens two panels:
  - Left: Modify HP, reinforcement refreshes, supplies (+50, +100, +1000, Clear), reset skill
    cooldowns, clear equipment, devices, or buildings, "Cancel the unlock limit", and **Modify
    Units: Upgrade, Demote, Remove Equipment** (for a selected unit).
  - Right: tabs for Devices, Buildings, and **Squad** (every unit, including Centurion, Death
    Knell, Mountain, and War Factory), plus **Adjust Unit Tech**.
  - Bottom left: Forced Move, Delete Unit, Clear All.
- Keys: WASD or arrows move the camera, the mouse wheel or Z/X zooms, Q/E/R/F rotate, and
  Ctrl+U upgrades every unit of the selected type.
- Dragging on the Squad palette scrolls the list; it doesn't place a unit.
- **Unlock Unit** (bottom right) opens a fixed grid of every unit by tier (I: Crawler … Sabertooth,
  II: Stormcaller … Wraith, III: Vulcan … Overlord, IV: War Factory, Mountain, Abyss) with tag
  filters. Clicking a unit adds its card to the shop (bottom right, with its price). This grid is
  the reliable way to get any unit.
- A drag from the shop card onto the board with synthetic mouse events placed nothing. That was
  probably because the camera didn't show my deployment area: after entering, the view pointed
  at a map edge, and neither keyboard zoom (X) nor the default view showed the grid. Next time:
  find a camera reset (or pan with WASD to the deployment zone, whose outline is dashed), then
  try click-to-select followed by click-to-place as well as dragging.

## What worked (second session)

- **Camera:** after entering, 15 mouse-wheel steps out (at the window center) show the whole
  deployment zone: a raised plateau with a dashed border, the Command Center, and a second
  structure. Clamped zoom should make this view repeatable.
- **Buying:** Unlock Unit, pick from the grid, then click the unit's shop card (bottom right).
  The unit appears at a default spot inside the zone (near the helipad on Railway Town) and
  supply drops by its price. Purchases per round are limited (the counter above the shop
  card).
- **Moving a new unit:** a drag from the unit (mouse down, glide, up) picks it up (a red
  dashed line follows the cursor). A click then drops it where the cursor is. After the first
  success, later attempts didn't move units, possibly because two new squads overlapped at the
  spawn point. Still unreliable.
- **Moving older units:** "Only newly recruited units can be moved." The fix (from the user):
  give yourself the **Redeployment** card to move units bought in earlier rounds.
- **Start** runs a round. **Menu, Save Replay** writes the sandbox game to the Replay folder
  (`2324_26-09-27__03-08-07-618_[jtk18]VS[Bot].grbr`). It's an ordinary replay (GameMode Normal,
  HP 50000, offline battle ID), so the app counts it as an offline bot game.
- **Calibration works:** a Crawler dropped at screen (768, 576) in the view above is recorded
  at `at=9.5,-25`. More points (one per purchase, all in one round) will give the projection.
- **A real fight:** use **Switch Sides** and add units to the opponent's board, or the round
  is just units walking into an empty base (from the user).

## How the controls work (from the user)

- **Move a unit:** click it, then click where it goes. Sometimes it needs a double click.
  **Right-click** deselects, and the unit snaps back to its last placement.
- **Cards, techs, and supplies:** Testing Grounds Tools can give you any card (including
  Redeployment, which lets you move units bought in earlier rounds), any tech, and money.
- **The opponent's board:** Switch Sides shows the same interface for the other player: unlock,
  buy, and place the same way.

## The `grounds` tool (crates/grounds)

`grounds place code.txt` sets up every side of a board code in a running Testing Grounds on
Railway Town, then leaves you on your own side. Press Start and it's a real fight.
`grounds plan code.txt` shows the screen targets without touching the game.

Verified 2026-09-27 against the saved replay with a two-sided code (Fortress, Marksman, and a
level-2 Crawler against Sledgehammer, Fang, and Arclight, placed with anchors). Every unit landed
where the code said, within the game's half-square snap, at the right level.

How it works:
1. **Camera:** 60 wheel steps out gives the same view every time, from either side (Switch Sides
   keeps the zoom; zooming fully out resets it). The whole zone is visible. Each half has a
   projective fit from 6 measured points (`RAILWAY_TOWN_SOUTH` and `RAILWAY_TOWN_NORTH`),
   with errors of 2 to 10 px.
2. **Spawning:** Testing Grounds Tools, Squad tab, click the unit. That's free and has no
   per-round limit (the shop allows 2 per round). The list has 35 units, three per row; scroll to
   the top, then 6 wheel steps per row. The tool checks that the panel really opened before
   scrolling, since over the map the wheel zooms instead.
3. **Finding it:** new units are tinted yellow-green (about RGB 160,180,90) and appear on the
   helipad (screen 1285,775 in either view). The new unit is the blob there that appeared or grew.
4. **Moving:** click the unit, click the target, double-click it, then right-click. It checks that
   the unit left the pad and arrived at the target, and retries up to 3 times. Drops on buildings
   (the Research Center, about board 14,-18 on the south) or on other units are refused.
5. **Levels:** select the unit, Tools, Upgrade once per level.
6. **Sides:** Switch Sides, then the same steps with the north calibration.

Gotchas: a cursor resting at a screen edge scrolls the camera, so the cursor parks over the
supplies counter. Stacked units on the pad are picked up together. Right-clicking cancels a carry.

Rotation and techs (verified 2026-09-27, same replay check):
- **Rotation:** carry the unit, click at the spot, **middle-click**, then double-click to confirm.
  R, F, Q, and E turn the *camera* instead (and switching sides doesn't undo that; leave and
  re-enter Testing Grounds to reset it).
- **Techs:** select the unit (a click picks it up), click its card's tech buttons, then
  right-click (it snaps back to where it was placed). The 4 buttons (screen 260/370/480/590,
  1453) follow the unit's equipped techs in replay `unitDatas` order, and each player has its
  own loadout: yours for the south half, Bot's for the north. `grounds` reads both from the
  newest replay. Each side has its own supplies, so it adds 5000 per side first.
- **Adjust Unit Tech** in the tools changes the loadouts themselves (which techs are equipped),
  not research on a unit.
- Hovering a unit in the Squad list shows its equipped techs and stats.

Tools tabs (right panel, top to bottom at screen x 2099): Squad (448), Specialists (560),
unit specialists (675), commander skills (783; Missile Strike, Shield Airdrop, Redeployment,
...), and Equipment (896).

Buildings and devices (verified 2026-09-29) are in the Squad tab, above the units: a Devices row
(Shield Generator, Sentry Missile, Missile Interceptor) and a Building section of 9 tiles, three
per row, that scrolls on its own (6 wheel steps a row, cursor at 2420,640; tiles at y 700).
Click a tile, then the spot: unlike ranked, where buildings stand in sockets beside the towers,
Testing Grounds builds anywhere on your half. `grounds buildings-check` places all 9 and reads
their IDs from the replay; `place` puts a side's buildings down after its units.

A unit card pinned from the Squad list (a click on a tile selects it) covers the top banner, so
the screen reader can't tell where the game is. Closing the tools removes it, and `enter` does
that when the left tools panel is open.

Items (verified 2026-09-28): the Equipment tab lists every item, grouped by rank, and it
scrolls. Clicking one puts it in your item inventory at the top left (screen 64,365). Click
it there, then click a unit, and the unit wears it (a badge shows over the unit). The same name
can appear in several ranks as different game IDs (Heavy Armor is in ranks 1 and 2), so
placing a code's items needs a map from catalog cell to item ID. That can be measured the way
calibration is: equip a different cell on each of a dozen units, fight, and read the IDs from
the replay. The hexagon slot on a unit's card shows its item; clicking it does nothing.

Placement details learned re-fighting real rounds (2026-09-28, 29):
- **The zone grows with the round:** in round 1 its front stops about 5 squares from the middle
  (a drop at y=-3 is refused, y=-6 is fine); from round 2 it reaches the front line, and flank
  columns open in round 3. Ranked round 1 already reaches the front line, so `verify` (and
  `replay`) fight empty rounds up to the code's `round` before placing.
- **The camera drifts by about a square** with fights and side switches, differently per half
  (2026-09-29: after empty fights one half's units all landed a square toward the middle, the
  other's exactly, and which half changed with the switching). It hid behind `verify`'s old
  one-square tolerance, now half a square. So `verify` keeps each half's empty view from just
  after clearing, and before placing on a half it lines up patches around the two towers and
  the spawn pad with that view (`camera_shift`) and aims that many pixels off. Matching the
  whole screen doesn't work: the lit zone grows by round, and it read a 13,22 px move as 168,18.
- **The front edge is 2 squares from the middle,** one farther back than ranked allows (checked
  2026-09-29 in rounds 2 and 3: a Fang aimed at y=-2 went to -3, and at y=2 on the north it was
  refused). A turned unit that pokes past it isn't pushed back on the north half: the game puts
  it down unturned. So `place` moves a unit straight back until its footprint clears the edge,
  and `verify` compares against the moved spot and prints a note.
- **Turning:** a middle click turns a carried unit, but it's a toggle, and a refused drop keeps
  the turn. So a unit is turned once, 4 squares behind its spot (where the unturned footprint
  fits even on the front line), then dropped. Turning it at the spot itself failed for a
  Sledgehammer on the front line: the unturned footprint didn't fit, so the drop was refused
  before the turn applied. Turning it at the spawn pad didn't turn it at all.
- **Camera after entering:** straight after entering, your own side's camera sits about a square
  off from where it is after switching sides and back. Calibration and verify both clear the
  board first, which does that round trip, so they see the same view.
- **Calibration:** `grounds calibrate --map NAME` drops 14 Crawlers per side, clear of the
  towers and the spawn pad. Each drop snaps to the grid, so a fit needs many points. When the map
  already has a calibration, it matches each drop to the unit recorded nearest where that
  calibration predicts (placement order breaks down with many units), and pools the points with
  earlier runs.
- **Spawn pad:** units are placed farthest from the pad first. A unit standing on the pad hides
  the next spawn (and a big model splits its green tile into pieces, so detection adds up all the
  green that appeared over the pad).
- **Towers:** Testing Grounds refuses drops right beside a tower (e.g. an Arclight at -14,-15 on
  the south half, or ±12,17 on the north), though ranked replays have units there. Either
  Testing Grounds blocks more around its towers, or the tower's model catches the click. A
  refused drop tries nearby spots (2 forward, 3 sideways, 4 forward) and reports where it went.
  2026-09-30/10-01, looking straight down (`flow::look_straight_down`: hold R, zoom out; the
  same view every time) with Night calibrated from the drawn tiles (fit within 9 px): each
  tower covers exactly 2×2 squares (its yellow outline shows while carrying a unit), and units
  stand flush against its sides and back, and the Research Center's front on the south half.
  The square flush in front of the Command Center can't be clicked: the click lands on the
  tower (its selection brackets reach about 1.25 squares out, and its click area farther) and
  the carry is dropped, with no red footprint. It's the same with the tower panned to the middle
  of the screen, so it isn't the camera's angle. The game centers a carried unit on the cursor
  whatever point it was grabbed by, so there's no aiming beside it. On the north half both
  towers' fronts fail the same way. Such a unit lands one square forward (the first nudge).
  A drag doesn't get around it either: pressing a square and a half away and releasing on the
  spot also drops the carry, and the Command Center shows its hover ring with the cursor about
  2.8 squares from its center (2026-10-01).
- **Tower keep-out (2026-10-01, Night, round 2):** a board with an Arclight flush against every
  face and corner of both towers (16 units, `from=cc-front` etc.), placed from the pad:
  straight down, the Research Center took 6 of 8 (not its left side or back-left corner) and the
  Command Center none; from the starting tilt (`GROUNDS_VIEW=tilted`, its own calibration in
  data/tilted.json), no better, and the Command Center again none. So the camera angle isn't
  it. `grounds click-map --tower cc` (one unit moved spot to spot) puts the Command Center's
  keep-out at about 4x4 squares (x -16 to -12, y -20 to -16 on the south half), against the
  2x2 that ranked replays show, and the Research Center's close to its 2x2. Units in front of
  the Command Center that the screen shows standing flush are recorded a square forward.
- **Fixed: aim off the tower (2026-10-01).** The user placed units flush against both towers by
  hand (a Sabertooth at -13.5,-14.5 in front of the Command Center, a Hound at 14,-15 in front of
  the Research Center), and the tool placed those exact two from straight down. The difference
  from the failing Arclight at -14,-15 is the click: 2 squares from the Command Center's center
  lands on the tower, 2.5 squares doesn't. A drop snaps to the grid, so near a tower `place` now
  also tries clicks 0.25 and 0.4 squares farther from it, one axis at a time (both at once at a
  corner overshot a square). The 16-unit flush board went from 6 exact to 13: still off are the
  Command Center's outer side (-16,-17, refused) and the Research Center's two inner corners
  (land a square farther out). A strict landing check (tile within half a square) made it worse:
  tiles beside a tower are partly hidden, so their centers read off.
- **Tower sides, 2026-10-02 (Railway Town and Night).** Earlier flush-board scores were too
  generous: `place` counted a drop as landed when the green near the target was there and the
  spawn pad's green had shrunk by a fifth. Beside a tower or other units there's always green
  near the target, and the pad's reading swings (78 to 355 for one Arclight), so units still
  on the pad were reported placed (three per 16-unit board). Now the pad must read empty
  (under 20; it reads 0 after every real move, 51 to 354 after refused drops).
  With that, the spots flush against a tower's side (x = +-12, y = -17 on the south half) are
  refused even with nothing else on the board: the tower takes the click. Only the aim 0.4
  squares away from the tower goes through, and it lands a square out (+-11). Corners and the
  front and back faces mostly land. Neither ordering by neighbours nor sliding the click along
  the face helped. `place` now says "placed by aiming off the tower (may be a square out)"
  for such units; `verify` gives the truth from the replay. (Units are placed farthest from
  the spawn pad first, then big models last; not in the code's order.)
  `grounds click-map --tower cc --reach 3 --step 0.5` on Night (one Arclight moved spot to
  spot) did take -16,-17, so how far a tower's click area reaches differs by map and side.
  Flush board with the fixed check, 2026-10-02: Railway Town 11/16, Night 10, Dusk 13,
  Forest Eye 10, Training Base 10, no unit left on the pad. Nearly every miss is the towers'
  inner sides (x = +-12, landing at +-11 or nudged) plus, on Night, Dusk and Training Base,
  the outer sides. Ranked replays allow those spots: of 94,476 units on 2,821 boards, about 700 stand flush against each
  side of each tower (inner and outer alike), so it's the click, not a game rule.
- **The side edges are a square in, too** (2026-10-01, Night round 2): Crawlers flush with
  x = ±30, which ranked allows, were refused; a square in, they land exactly. The back edge is
  ranked's. So `place` moves units in from x = ±29 (`SIDE_EDGE`), pushing packed neighbours
  along, as it does at the front edge.
  Debugging: `GROUNDS_DEBUG=dir` saves each drop's screenshot and the frame just after the
  drop click (`carry*.png`), which shows the footprint the game tried.

## Re-fighting real rounds

`grounds replay FILE --round N` sets up round N of a real match (the last by default) on a
fresh Testing Grounds board on the same map, fights it, and prints each half's result next to
the real one. It leaves out, and lists, what Testing Grounds can't reproduce: techs not in the
Testing Grounds loadout, units it can't click to, and items. Then it lists every unit that
landed somewhere other than planned.

First runs (Railway Town, round 2, 10 units against 9): your half lands exactly except
units beside the towers. The opponent's half packs units edge to edge in front of its towers,
so a few get nudged, and the outcome flipped. A setup that close still isn't the same fight:
Testing Grounds HP is 50000 per side, so only the winner is compared, not the damage.

## On macOS

`grounds` drives the native macOS build of the game too (`crates/grounds/src/mac.rs`, the same
`Game` interface as `win.rs`; `driver` names whichever one the platform has). First set up on
2026-11-08 on a 16-inch MacBook Pro (3456x2234 display, macOS 26, game 2.0.0.5.2340).

What it needs:

- **Permissions.** The terminal that runs `grounds` needs Accessibility (to send mouse and key
  events) and Screen Recording (to capture the window), under System Settings, Privacy &
  Security. Without Accessibility macOS drops the events without an error, so `Game::find`
  refuses to start. Accessibility lets anything run in that terminal control the Mac; turn it
  off when you're done if you like.
- **Increase max camera height** (Settings, Control) must be on, with its slider at the right
  end. Off, the camera stops zooming out about 1.6 times closer (towers about 46 px a square
  apart in reference pixels instead of 28.5), the zone doesn't fit on screen, and the PC's
  calibrations don't apply. With it on, the towers sit within a square of where the Railway Town
  calibration puts them, straight after entering.
- **A 16:10 picture.** Screen positions are measured on 2560x1600. Fullscreen at 1920x1200
  (the game's setting) fills the 1728x1080 points below the notch; the game's window is
  1728x1084 there and the picture is letterboxed by 2 points above and below (4 black pixel
  rows at each edge of a capture). The driver splits small surpluses like that evenly, and takes
  a larger one (28 points or so) as a title bar, for a windowed game at 16:10.
- **A release build** for real runs (`cargo build --release -p mechabellum-grounds`). The first
  `verify`, in a debug build with an earlier capture that resized in Rust, took 18 minutes,
  most of it CPU. In release, `place` sets up a 7-unit, two-sided board in about 4 minutes,
  nearly all of it the tool's own waits.

How it differs from Windows:

- Input is CoreGraphics events posted to the HID tap, in points; the game turns them into its
  own pixels. Moves made while a button is down are sent as drags, and double clicks carry a
  click count of 2. Camera keys use macOS key codes.
- Screenshots come from `CGWindowListCreateImage` on the game's window (deprecated, but present
  and working on macOS 26), cropped to the picture and scaled to 2560x1600 by CoreGraphics while
  drawing, in sRGB.
- One wheel step is one line of scroll. 60 steps out reach the zoom limit, as on Windows, and the
  Squad list's 6 steps a row hold: a Fire Badger (row 4) spawned from it, checked on its card.
- A right-button pan moves the map about 40% of the drag (once measured), so `grounds pan` needs
  a longer drag than the distance wanted.
- A fullscreen game lives in its own Space; `find` brings it forward with `open -b` when another
  window is in front.
- The screen reader (`screen::classify`) needed no changes: it read the main menu, the menu, and
  the sandbox correctly from the first captures.

Checked on 2026-11-08: `enter` from the main menu and `enter --fresh` from the sandbox; `verify`
on Railway Town with 3 units on the south half and 2 on the north ("OK: the replay matches the
code", every unit within the half-square snap, using the Windows calibration unchanged); `place`
on five more two-sided boards, including researching Ignite on Fangs (the card shows it bought,
and supplies dropped by its price); `shot`, `which`, `zoom` and `pan`. Not run on the Mac yet:
`calibrate`, `replay`, `items-check`, `buildings-check`, `grant`, `click-map`, the tilted view, and
any map other than Railway Town. The Windows build wasn't compiled for this change (no Windows
target here); on Windows only the `driver` alias is new.

## In the desktop app, and stopping a run (2026-11-16)

The whole-board runs moved from the program into the library (`crates/grounds/src/setup.rs`) so
the desktop app can run them: "Set up in Testing Grounds" on a match's board sets up the round
shown, and can fight it and compare. A real match's round goes through `Options::leave_out`,
which sets up what Testing Grounds can reproduce and lists the rest (a ranked map becomes
Railway Town; techs not equipped and items not in the Equipment tab are dropped), as `grounds
replay` always did. That filter now runs after an empty round has recorded the loadouts, so a
first run no longer drops every tech.

A run can't be interrupted from the keyboard while it owns the mouse, so the drivers stop
themselves: before each input they compare the cursor with where they last put it (6 points or
pixels of slack, read twice 60 ms apart, since posted moves land late), and if it moved they
send nothing more and fail the next screenshot with `stop::Stopped`. A held button is let go
first. The app's Stop button sets the same flag. Calibrations are compiled in, so the app and
a moved binary need no data folder.

Not yet run against the game: the stop, the app panel, and the Windows build of any of it.

## Research Center blueprints

Clicking your Research Center (about 1642,962 in the straight-down view on Railway Town) opens its
panel at the bottom left: an upgrade, and five blueprints at y 1512, x 282 (Sticky Oil Bomb, 150
supplies), 394 (Field Recovery), 508 (Mobile Beacon), 622 and 736 (the two enhancements). A bought
blueprint's skill appears at the top left (64,360, where granted items also wait). Sticky Oil Bomb
aims with two clicks: where the line starts, then which way it runs; it's about 16 squares long
and fires a few seconds into the fight. The skill can be cancelled until the round starts.

## Mechanics checked

**Does a Fang's Ignite set off Sticky Oil? No** (2026-11-08, game 2.0.0.5.2340, Railway Town,
round 1, one Testing Grounds session per trial). Four Fang squads stood behind a Sticky Oil Bomb
line laid along their front edge, and the Bot's units charged through the oil into them.

| Trial | Your side | Bot | The oil |
|---|---|---|---|
| Test | Fangs with Ignite | 3 Crawler squads | intact through the fight and into round 2 |
| Test, again | Fangs with Ignite | 2 Rhinos, 1 Crawler squad | intact through the fight and into round 2 |
| Control: no fire | Fangs, no techs | 3 Crawler squads | intact through the fight and into round 2 |
| Control: no fire | Fangs, no techs | 2 Rhinos, 1 Crawler squad | intact through the fight and into round 2 |
| Control: fire | 2 Fire Badger squads | 3 Crawler squads | burned off within a few seconds of the flames reaching it, leaving embers; gone in round 2 |

The fire control shows burning oil is easy to see in these captures (the game's loading tip says
the oil "can be ignited with fire"). That Ignite really burned units standing in the oil comes
from the Rhino pair: with Ignite, the Rhinos carried orange flames on their hulls and both died
about 25 capture frames sooner than without it (frames 38 and 40 against 62 and about 66, at
0.51 s a frame: some 12 to 13 seconds), in the same setup, while standing in the oil the whole
time. The oil landed at the same frame in both runs. So Ignite's burn on a unit in the oil doesn't light it; a
flamethrower's fire does. One sample per condition (two for Ignite); untested: other units'
Ignite techs, Incendiary Bomb, and units ignited elsewhere that then walk into the oil.

## Plan

1. **Driving:** a small Windows driver (the tester crate already has window lookup, clicks,
   and screenshots) with named UI targets, as fractions of the window size so they survive
   other resolutions.
2. **Camera:** reset the camera to a known view, then fit board squares to screen pixels:
   place a unit at a few screen points, start the round, and read the unit positions back
   from the replay Testing Grounds saves (offline games are saved with `__` battle IDs). A
   fitted projection from four or more points maps any `at=x,y` to a screen point.
3. **Placing:** for each side (Switch Sides between them) and each unit: add it from the
   shop, drop it at the mapped point, rotate if `rotated`, click Upgrade `level - 1` times, and
   set techs with Adjust Unit Tech.
4. **Checking:** start the round, read the saved replay, and compare its round-1 armies with
   the board code (`Board::from_round` gives a code to diff against).

Safety: only click targets that were recognized on screen; stop if an expected screen isn't
found. Never touch Multiplayer, Play, Join, or Spectate.
