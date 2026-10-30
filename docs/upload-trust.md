# Upload trust: keeping the shared stats from being poisoned

Status: proposal (2026-09-26). Applies to the upstream server once desktop clients sync replays.

## The problem

Clients upload replays, and a replay is just a file on the player's machine. There is no public
server-authoritative source to check it against: Game River doesn't expose replays by battle ID.
The game's own `Player.log` could corroborate a match, but it's local and just as editable. So
**we can't prove any single upload is genuine.** Someone could:

- edit a real replay (flip a result, add a unit, boost a win rate)
- fabricate replays from scratch
- make many accounts to outweigh honest players (Sybil attack)
- upload the same games repeatedly

Other trackers (17lands, HSReplay) parse official client logs and publish no anti-tampering
scheme. The crowdsourcing literature converges on the same answer we have to take: **make faking
expensive, make its effect bounded, and make it reversible.**

## Layers

### 1. Identity: make accounts cost something

- **Sign in with Steam** (OpenID). One Steam account gets one uploader identity, its devices'
  keys counted as one uploader. Built 2026-10-29 (README, "Sign in with Steam"); hand-made keys
  still work. Steam can't say which Mechabellum account a Steam account plays, so binding stays
  first come; each Mechabellum account can be bound by one active uploader.
- Use the Steam Web API to gate new uploaders on things that are slow to fake: account age,
  Steam level, Mechabellum ownership and playtime. With the free edition (2.0), ownership alone is
  cheap, so age and playtime matter more.
- **Bind each uploader to one Mechabellum account ID** (the `userid` in replays and
  `Player.log`), claimed on first sync. Uploads must include that player. Only
  games you played in are accepted, so one person can't pose as the whole community.

### 2. Validity: reject what can't be real

Cheap checks on every upload, all things the parser already understands:

- the version exists and the battle ID's date falls within that version's lifetime
- every unit, tech, specialist, and card ID is valid for that version
- structure: each round's army matches that round's fight report exactly (it always does in real
  replays), round count ≤ MaxRound, HP only moves in ways fight results explain
- XP and level agree with the known thresholds; supply spent fits supply available
- the same player isn't in two overlapping matches; battle IDs don't repeat

A hand-edited replay usually breaks one of these. A fully consistent fabrication takes real effort
per game, which is the point.

### 3. Corroboration: two witnesses beat one

Every match has two players. When both upload the same battle ID, compare a hash of the shared
content (units, positions, actions, results; not names, which one side has stripped). Matching
copies mark the match **corroborated**; mismatches flag both uploaders for review.

- Stats pages can offer "corroborated games only" as a trustworthy view.
- Corroborated games weigh more in the default view.

### 4. Aggregation: no single uploader moves the numbers

- **Cap each uploader's weight** in any stat bucket (for example, no one account above 1–2% of a
  unit's games in a patch). An attacker then needs many accounts, which layer 1 makes costly.
- **Robust aggregation**, per the crowdsourcing literature: group results by uploader, then take
  the median of group means rather than one global mean. A few poisoned groups can't drag it far.
- **Outlier detection**: compare each uploader's results to everyone else's (win rate per unit,
  per patch). Flag accounts many standard deviations out, for review rather than automatic
  deletion.

### 5. Reversibility: undo a bad actor completely

- Every match already records `uploaded_by`, and stats are derived data rebuilt from stored
  replays (`reindex`). Banning an uploader deletes their matches and rebuilds, leaving no residue.
- New uploaders' data can sit in quarantine (stored, excluded from public stats) until it's
  corroborated or the account has a track record.

## Privacy interplay

Opponents' names are stripped on the client before upload. The server then keeps no names at all,
and replaces every account ID with a keyed HMAC of it (README, "Pseudonyms"); a plain hash would
be reversible, since names are public and account IDs are a small, counting space. Its responses
number players within each match, round ratings, and refuse per-player filters, so a known match
can't be followed to a player's others. That doesn't weaken any layer: corroboration hashes use
each player's place in the replay, not names or IDs, and binding uses the keyed IDs.

## Suggested phases

1. **With desktop sync** (now): per-uploader keys bound to one Mechabellum account ID, the validity
   checks, `uploaded_by` on every match, and a purge-and-rebuild command.
2. **With the public website**: Steam sign-in with account-age and playtime gates, corroboration
   flags and the corroborated-only view, per-uploader caps.
3. **Once there's volume**: robust aggregation and outlier review.

## Hostile content (injection)

Replays, ratings, filters, and any future shared format (such as the board markup for Testing
Grounds) are untrusted text. Rules, each backed by a test:

- **SQL:** values only ever reach SQLite as bound parameters (`?1`, `params![]`). The few
  queries assembled with `format!` splice in compile-time table and column names only
  (`&'static str`), never data. `test_sql_in_replays_is_just_text` stores a player named
  `Robert'); DROP TABLE matches; --` and a quoted battle ID, and checks both come back verbatim
  and that hostile filters match nothing.
- **XML:** the parser does not expand external or nested entities, so a replay can't read local
  files or balloon in memory (`test_entities_are_not_expanded`). Upload bodies are capped in
  size before parsing.
- **HTML:** the app and website insert replay text with `textContent`, never `innerHTML`.
- **New formats** are parsed into typed values with limits (lengths, counts, known IDs) before
  anything else touches them.
