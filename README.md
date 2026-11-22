# WorldShard

An open-world fantasy game set on a broken chunk of Earth floating in space.
Thor's hammer broke the Earth apart; this shard is one of the pieces. It sits in
a half-sphere bowl of water (the **Rim Sea**) ringed by the **Cloudwall**, where
all of the island's weather is born. Mount Everest and Mount St. Helens stand in
the middle, and six regions with their own native peoples, nature and geography
spread out from their base. Content and items are player-driven.

![Top-down view of the generated island](docs/images/world_preview.png)

## Scale

- **Island:** the land area of Australia (about 7.7 million km²), so about 3,100 km across.
- **Rim Sea:** three island widths of water from the shore to the Cloudwall.
- **Peaks:** five times their real heights, so they tower over the island: Mount Everest 44,245 m, Mount St. Helens 12,745 m (with its crater open to the north), in a range of lesser mountains.
- **Regions' landforms:** North rugged rocky highlands; Northeast volcanic plateau with mesas and dune seas; Southeast rolling hills; South steep hills and ravines; Southwest open plains; Northwest a low marshy basin. The region borders rise as ridgelines.
- **Clock:** a game day is a real day, so crossing a region on foot takes 3–5 game days.
- **Fly-through mode (testing):** press F or tap **Fly**. E/Q or **Up**/**Down** change height. R or the speed button cycles 30 m/s, 300 m/s, 3 km/s and 30 km/s.

## The six regions

| Region | Nature |
|---|---|
| North | Rock mines |
| Northeast | Lava flows and desert |
| Southeast | Mid forest with fields, and the inlet sea |
| South | Dark forest |
| Southwest | Grasslands |
| Northwest | Lakes and swampy forest |

Full world design: [docs/map-and-world.md](docs/map-and-world.md).

## Repository layout

| Path | What it is |
|---|---|
| `src/worldgen/` | **Rust** world generator, no dependencies. `island.rs` is the design-map recipe (shape, regions, colours); `terrain.rs` samples the full-size world anywhere in metres, with real peak heights and ground-level detail. |
| `src/lod.rs` | Level of detail: a quadtree of terrain chunks, 256 m near the camera and coarser further out. |
| `src/meshgen.rs` | Mesh data any renderer can upload: terrain chunks (with skirts, water and lava), trees / bushes / rocks / mine entrances in 128 m cells, grass in 32 m cells, the Cloudwall and the sky dome. |
| `src/sky.rs` | Day and night: sun and moon direction, light, sky and fog colours for any time of day. |
| `src/save.rs` | The save format (read and write), no dependencies. |
| `game/` | **The game** in [Bevy](https://bevyengine.org) 0.16: draws the world, walking / running / swimming / jumping, mouse and touch controls, day/night, region banners, compass, autosave. `game/web/index.html` is its web page. |
| `.github/workflows/web.yml` | Builds the game for the web (WebAssembly) on every push and publishes it to GitHub Pages from `main`. |
| `src/bin/mapgen.rs` | Checks the full-size world (peak heights, chunk counts) and writes a shaded preview (`world_preview.ppm`, 4 km per pixel) and a 16-bit heightmap (`world_height16.pgm`, value − 10,000 = metres). |
| `web-prototype/index.html` | The first playable version: a single-page WebGL 2 web app. Walk the island in first person on phone or desktop, swim, day/night cycle, Cloudwall barrier, compass, minimap and region banners. Reference for the Rust port. |
| `tools/island-map.html` | The illustrated top-down design map (canvas shaded relief) built from the same generator. |
| `docs/` | World design, free asset sources, prototype notes. |

## Running

Play in the browser: **https://christianhreed.github.io/WorldShard/** (after
GitHub Pages is switched on: repo Settings → Pages → Source: *GitHub Actions*).

```sh
# The game on desktop
cd game && cargo run --release

# The game for the web (needs the wasm32 target and wasm-bindgen-cli)
cd game && cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir dist --out-name worldshard \
  target/wasm32-unknown-unknown/release/worldshard-game.wasm
cp web/index.html dist/   # then serve dist/ with any static web server

# World generator preview + mesh sizes (about 0.4 s)
cargo run --release --bin mapgen

# Web prototype: open the file in a browser (needs WebGL 2)
open web-prototype/index.html
```

## Saving

Saves are small JSON files the player keeps wherever they like — the plan is
Google Drive, without any connector or API: the game offers the file through
the device's save / share menu, and loads it back through the file picker.
Format:

```json
{"game":"WorldShard","v":1,"x":0,"z":0,"yaw":0,"pitch":0,"hours":9,"day":1,"visited":["Southwest"],"playSec":0,"savedAt":0}
```

The game saves itself in the browser as you play (a few seconds after you
move, and when you reach a new region). **Save file** on the game page hands
that save to you as a file (on phones through the share menu, so you can pick
Google Drive); **Open save** loads one back.

## Status and next steps

- [x] World design, regions and map
- [x] Web prototype you can walk around
- [x] World generator ported to Rust (verified identical to the JS version)
- [x] Rust game in Bevy, built for the web with WebAssembly by GitHub Actions
- [x] Terrain, lava, sea, trees/rocks/mines, Cloudwall, day/night and walking ported to Rust
- [x] Full-size world (Australia-sized island), streamed terrain, fly-through mode
- [ ] Breath of the Wild look: soft two-tone lighting, wind, shadows, painted textures
- [ ] Minimap in the Rust version
- [x] Real trees, bushes, rocks and small plants near the player (Quaternius Stylized Nature MegaKit, CC0; see [CREDITS.md](CREDITS.md))
- [ ] Mine entrances and a far-distance look for trees that matches the real models
- [ ] Native inhabitants, items and player-built content

Note: the save-file feature in `web-prototype/index.html` has not been tested yet.
