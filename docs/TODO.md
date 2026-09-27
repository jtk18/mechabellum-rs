# Work list

Last updated 2026-09-26. Newest decisions are in `ORIENTATION.md` at the scratchpad root.

## In progress: sharing matches (trust phase 1)

Done: server side (validation, key binding to one account, name hiding, `/missing`, ratings,
purge), MMR capture from `Player.log`, and the desktop sync worker (`sync.rs`, off by default).

1. **Sync settings UI** in the desktop app: server URL, upload key, on/off switch, progress
   ("Shared 340 of 1,053"), rejected matches with reasons, and a "Sync now" button. Commands
   exist: `sync_settings`, `set_sync_settings`, `sync_now`, `sync_status`, plus `sync-status`
   events.
2. **End-to-end test of sync** against a local upstream server (`serve --require-auth` on
   another port, `keys add`). Check that:
   - all online matches arrive and the 10 offline bot games come back rejected
   - opponent names are hidden server-side, and ratings are attached
   - a restart resumes without resending
   - a second key for the opponent fills in their name
3. **Check tonight's games** (the user is playing now): new replays ingest and validate, and
   `Player.log` ratings land in `player_ratings` for them.
4. **Show MMR** in the match list and on the board (both players' elo at match start).

## Stats

5. **Win rate by opponent MMR band.** Possible once ratings accumulate.
6. **Group builds into seasons or patches** in the heatmap. Many builds have under 10 matches.
7. **Tech icons in stats tables by (unit, tech ID)** instead of tech name. The same name can
   have different icons per unit.
8. **Remaining art:** Star Expedition supply and equipment specialists (Supply Crate, Equipment
   Expansion, Mass-produced …), plus equipment/item icons.
9. **Verify `towerStrengthenLevels`** order: index 0 is assumed to be the Command Center and
   index 1 the Research Center.

## Board

10. **Command Center and Research Center positions** on the map. Not in replays; per-map layouts
    are needed.
11. **Verify inferred footprints** against the game and correct `game_data.rs`.

## Platform and distribution

12. **Replace the AssetStudio CLI** (needs .NET 9) with native Rust extraction before
    distributing, so players don't need .NET.
13. **CI:** a GitHub Actions matrix (Windows, macOS, Ubuntu) to build and test the workspace and
    the Tauri app.
14. **Tauri bundles and installers**, start on login (optional), and single instance.
15. **Verify macOS and Linux (Proton and native)** replay and log folder detection with real
    installs.
16. **Verify the Docker build** on the Ubuntu server.

## Server and website (trust phases 2 and 3; see docs/upload-trust.md)

17. **Steam sign-in** with account-age and playtime gates, replacing bare upload keys for
    members.
18. **Corroboration:** when both players share a match, compare hashes of the shared content
    (excluding names). Add a "corroborated only" view and flag mismatches.
19. **Per-uploader caps** in aggregates; median-of-means aggregation; outlier review.
20. **Public website:** stats pages and per-player pages (players shown pseudonymously unless
    they share).

## Tester track

21. **Read unlocked techs** per unit from `ProjectDatas/GameData.grdb` instead of OCR.
22. **Automate Testing Grounds** setup from replay army snapshots (the drill-down already has
    every unit's position, level, and techs).

## Housekeeping

23. Delete the merged `version-compat` and `workspace` branches, locally and on GitHub.
24. Fix the older clippy warnings in the parser (`len() == 0`, `write!` with `\n`).
