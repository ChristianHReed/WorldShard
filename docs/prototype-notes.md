# WorldShard — Web Prototype Notes (v0.3)

`web-prototype/index.html` is the first playable version, built Nov 17, 2026. The game is moving to Rust; this prototype stays as the reference for the port.

## What it is
- A **web app** you walk around in, in first person, on phone (touch joystick + drag to look) or desktop (WASD + mouse drag, Shift run, Space jump).
- Runs in the browser with **WebGL 2**, no engine and no downloads. Single page.
- The terrain comes from **the same generator and seed as the island map** (and `src/worldgen` in Rust), so the 3D world matches the map.

## Scale
- 1 map pixel = 4 m. Island is ~2.9 km across; walking 5.5 m/s, running 13 m/s. Tallest peak ~580 m.
- Much smaller than the design target (3–5 days to cross a region; two weeks to sail to the clouds). Scale is one number to change later.

## What's in each region (placeholder art)
| Region | In the prototype |
|---|---|
| North — rock mines | Grey rugged rock, scattered boulders, 4 mine entrances with rails |
| Northeast — lava flows & desert | Sand, glowing lava channels in dark crust, basalt and sandstone rocks |
| Southeast — mid forest & fields | Patchwork fields with hedges, forest patches |
| South — dark forest | Dense dark pines and broadleaf trees |
| Southwest — grasslands | Open grass, few trees and bushes (spawn point) |
| Northwest — lakes & swampy forest | Pools, small lakes, olive forest, dead trees |

## World rules implemented
- **Day/night:** 1 game hour = 1 real minute; sun rises in the east, moon and stars at night. Below the horizon is open space (the shard floats).
- **Cloudwall:** ring of clouds around the Rim Sea; an invisible barrier stops the player before it (late-game only), with a message.
- **Swimming** in the Rim Sea.
- Region name banner on entering each region; compass; minimap.

## Saving
- **Save file (Google Drive, no connector):** Menu → "Save file" offers a JSON file through the device's save/share menu so the player can put it in Google Drive; "Open save file" loads one back through the file picker. If the page can't offer a file, it shows the save text with a Copy button. *Not tested yet.*
- **Quick save:** autosaves position, facing, time, day, visited regions and play time (every ~12 s while moving and when the tab is hidden) to the player's private slot when running inside Claude, otherwise to browser storage.

## Known limits
- Trees, rocks and mines are simple built-in shapes until CC0 assets go in.
- No tree collision, no rivers, no natives, no items or player-built content yet.
- No third-person character, sound, or weather spawning from the Cloudwall yet.
