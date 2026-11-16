# grounds

Sets up a Mechabellum board in the game's Testing Grounds by driving the game: it finds the
window, clicks through the menus into Solo, Testing Grounds, and places both sides' units with
their levels, turns, techs and items, plus buildings and devices. It can then fight the round and
check the replay the game saves against the board. Windows and macOS.

The board comes as a board code ([docs/board-code.md](../../docs/board-code.md)): a short text
block of who stands where. The desktop app makes one for any round of a stored match.

## Two ways to run it

**From the desktop app.** Open a match, pick a round, and press **Set up in Testing Grounds**. The
panel says what's about to happen, asks macOS for its permissions, and shows progress. Tick "Then
fight the round" to have it fight and compare the result too. Nothing else needs installing.

**From the command line**, with a board code in a file:

```sh
cargo build --release -p mechabellum-grounds
grounds enter                # from the main menu or the lobby into the sandbox
grounds place board.txt      # set up the board in the sandbox that's open
grounds verify board.txt     # enter on the code's map, set it up, fight, compare the saved replay
grounds replay match.grbr --round 3   # re-fight a real round and compare the outcome
```

Release builds run much faster than debug ones; use them for real runs. `grounds --help` lists
every command, including the measuring tools used to build it (`calibrate`, `click-map`,
`items-check`, `buildings-check`, `shot`, `which`, `where`).

Each [release](https://github.com/jtk18/mechabellum-rs/releases) also carries the command-line
program as a zip for Windows and macOS. The Mac build isn't signed: clear the download quarantine
once (`xattr -dr com.apple.quarantine <the unzipped folder>`) or macOS refuses to open it.

## Before a run

- **Leave the mouse and keyboard alone while it works. Moving the mouse stops it**: every click
  first checks that the cursor is where the last one left it, and once it isn't, no more input is
  sent and the run ends with "stopped: the mouse moved". The desktop app's Stop button does the
  same.
- In the game's Settings, Control, turn on **Increase max camera height**, with the slider at the
  right end. The calibrations assume the camera's farthest zoom; without it the deployment zone
  doesn't fit on screen.
- The game's picture must be **16:10** (fullscreen at 1920x1200 or 2560x1600, or a 16:10 window).
  Screen positions are measured on 2560x1600 and scaled.
- **macOS:** the app that runs it needs **Accessibility** (to send clicks and keys) and **Screen
  Recording** (to see the game), under System Settings, Privacy & Security. That's your terminal
  for the command line, or Mechabellum Stats for the desktop app. Without Accessibility macOS
  drops the clicks silently, so `grounds` refuses to start. Screen Recording takes effect after
  that app restarts.

Getting into Testing Grounds goes from the main menu through Standard Versus to the Solo tab, and
never presses Play: each step of the way first checks which screen the game shows.

## What it can set up

| | |
|---|---|
| Maps | Testing Grounds' five: Railway Town (and its Dusk and Night versions), Forest Eye, Training Base |
| Rounds | 1 to 12: it fights empty rounds until the code's round, as the deployment zone grows with the round |
| Units | Every unit in the Squad list, at any level, turned or not |
| Techs | The four each unit has equipped in Testing Grounds (yours for the south half, the Bot's for the north) |
| Items | Every item in the Equipment tab |
| Buildings, devices | The Squad tab's Building tiles and Devices row |

Testing Grounds' zone is a square smaller at the front and the sides than a ranked game's, so a
unit on the very front line moves back a square; `verify` lists those as notes, not mismatches.

A round of a real match often holds more than Testing Grounds can reproduce. The desktop app and
`grounds replay` set up what they can and list the rest under "left out": a map Testing Grounds
doesn't have (Railway Town stands in; the towers stand in the same places on every map, but the
ground between differs), techs not equipped in Testing Grounds, items not in the Equipment tab.
`place` and `verify` refuse such a code instead.

**Not yet:** choosing which four techs a unit has equipped (Testing Grounds Tools, Adjust Unit
Tech). Until then a tech must already be equipped to be researched.

## How it works

No OCR. It recognizes screens by the mean color of a few small regions (`screen.rs`), finds a
unit it just spawned by its green "bought this round" tint, and maps board squares to screen
pixels with a projection fitted from measured points (`calibration.rs`). The points were measured
by dropping units at known screen positions and reading where the saved replay recorded them.

Techs are clicked by position: a unit's card has four tech buttons in the order the replay lists
its equipped techs, and a Testing Grounds replay records that order for each side. So `grounds`
reads the newest Testing Grounds replay first (fighting an empty round to make one if there's
none).

Every Testing Grounds map's calibration is compiled in from `data/`. `grounds calibrate --map
NAME` measures a map again and saves it to `top-down.json` in this crate's `data/` folder when
run from a checkout, or next to the program otherwise; a saved map takes precedence over the
compiled-in one.

The details, measurements and checks are in [docs/testing-grounds.md](../../docs/testing-grounds.md).

## Checked so far

- **Windows:** `verify` on all five maps; the item and building tiles were checked there too
  (September and October 2026).
- **macOS:** `enter`, `place` and `verify` on Railway Town (November 2026). `calibrate`, `replay`
  and the other maps haven't been run on a Mac yet.
- The mouse-move stop and the desktop app's panel are new and haven't been run against the game
  yet on either platform. The Windows build hasn't been compiled since the macOS driver arrived.

## As a library

`mechabellum_grounds::setup` holds the whole-board runs (`set_up`, `verify`, `place`) that the
program and the desktop app share. They report progress through a `Report` (`Print` prints it)
and end with an error that `stop::is_stop` recognizes when the user stopped them. The `cli`
feature (on by default) builds the program; the desktop app turns it off.

Environment variables, for development: `GROUNDS_VIEW=tilted` keeps the game's starting camera
tilt (with its own calibrations) instead of looking straight down; `GROUNDS_DEBUG=<dir>` saves a
screenshot of every drop; `GROUNDS_STOP_AFTER=<n>` ends `click-map` after n probes.
