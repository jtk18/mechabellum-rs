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
