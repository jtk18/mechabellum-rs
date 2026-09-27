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
- Dragging on the Squad palette scrolls the list; it doesn't place a unit. Units probably go
  through the normal shop (bottom right: unit cards with prices and "Unlock unit"). The palette
  may add a unit type to the shop. Still to find out.

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
