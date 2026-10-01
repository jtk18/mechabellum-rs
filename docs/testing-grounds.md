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
