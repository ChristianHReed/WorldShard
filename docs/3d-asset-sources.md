# WorldShard — Free 3D Assets & Tools (v0.1)

Research date: Nov 17, 2026. Always re-check the license on the download page before shipping.

**License key**
- **CC0** = public domain. Use commercially, modify, no credit needed.
- **MIT** = open-source code. Keep the license file with the code.
- **CC-BY / CC-BY-SA** = must credit the author (SA = your changes must use the same license). Avoid "NC" (non-commercial) if the game will be sold.

Note: this list was researched before the move to Rust. The art assets (glTF/FBX/textures) work in any engine; the Godot plugins listed under terrain, water and sky are references for what to build or find Rust equivalents of.

---

## Art style
Don't mix the two families below or the world will look patchy.
- **Stylized / painterly** (closest to the cliff and Anton Fadeev references): Quaternius, KayKit, Kenney.
- **Photoreal / scanned** (closest to the satellite-style map reference): Poly Haven, ambientCG.

---

## Terrain — island shape, regions, the two peaks
| Tool | What it does | License | Link |
|---|---|---|---|
| Terrain3D (Godot 4) | High-performance editable terrain; sculpting, texture painting, foliage instancing; imports heightmaps | MIT | github.com/TokisanGames/Terrain3D |
| HTerrain (Godot) | Heightmap terrain with a built-in generator | MIT | github.com/Zylann/godot_heightmap_plugin |
| Tangram Heightmapper | Grayscale heightmap of any real place — use for real **Mount Everest** and **Mount St. Helens** | Open source (code) | tangrams.github.io/heightmapper |
| 16-bit PNG Heightmap Generator | Same idea, 16-bit output (smoother than Tangram's 8-bit) | Free web tool | manticorp.github.io/unrealheightmap |

## Ground textures (all CC0)
- **ambientCG** — ambientcg.com — 2,000+ PBR materials (rock, sand, mud, moss, soil, lava).
- **Poly Haven** — polyhaven.com — scanned textures, HDRIs and models.
- **ShareTextures** — sharetextures.com — includes CC0 lava/magma materials with emissive maps.

## Assets by region
| Region (design) | Assets | License |
|---|---|---|
| **North — rock mines** | Mine Object Collection (tracks, carts, picks) — loafbrr.itch.io/mine-object-collection · Free Mine Assets Pack (rubberduck) — opengameart.org/content/free-mine-assets-pack · rocks from Stylized Nature MegaKit or Poly Haven | CC0 |
| **Northeast — lava flows & desert** | Quaternius Low Poly Desert assets — opengameart.org/content/low-poly-desert-assets · ShareTextures lava | CC0 |
| **Southeast — mid forest & fields** | Quaternius Crops Pack (100+ crops, 5 growth stages) — opengameart.org/content/lowpoly-crops-pack · trees from MegaKit / KayKit Forest | CC0 |
| **South — dark forest** | KayKit Forest Nature Pack (200+ unique models) — kaylousberg.itch.io/kaykit-forest · Quaternius Stylized Nature MegaKit (40 trees, 27 rocks, plants, grass) — quaternius.itch.io/stylized-nature-megakit | CC0 |
| **Southwest — grasslands** | MegaKit grass/flowers · Kenney Nature Kit (330 assets) — kenney.nl/assets/nature-kit | CC0 |
| **Northwest — lakes & swampy forest** | Nature Props (Fertile Soil Productions; includes cattails) — fertile-soil-productions.itch.io/nature-props · mud/moss textures from ambientCG / Poly Haven · **Gap:** no good free CC0 swamp kit found; lily pads/reeds may need to be made | CC0 |

## Water — Rim Sea, inlet sea, lakes
- **Boujie Water Shader** (Godot 4.1+, MIT) — ocean mesh with LOD, Gerstner waves, shore foam — github.com/Chrisknyfe/boujie_water_shader
- **GodotSSRWater** (MIT) — transparent water with reflections/refraction — github.com/marcelb/GodotSSRWater
- **Gap:** the half-sphere bowl edge where the sea meets the Cloudwall will need custom work.

## Cloudwall, sky, sun & space
- **SunshineClouds2** (Godot, MIT) — volumetric clouds you can fly through — github.com/Bonkahe/SunshineClouds2
- **Sky3D** (Godot 4.3+, MIT) — day/night cycle, rotating sun/moon/stars — github.com/TokisanGames/Sky3D · its star-map asset requires crediting the author.
- **Space backdrop:** Kenney Skyboxes (CC0) — kenney.nl · Seamless Space Backgrounds (64 images; CC0) — screamingbrainstudios.itch.io/seamless-space-backgrounds

## Characters, animals & movement
- **Quaternius Universal Base Characters** (6 base models, 20 hairstyles; CC0) — quaternius.itch.io/universal-base-characters
- **Quaternius Universal Animation Library 1 & 2** (120+ and 130+ retargetable animations; free tier CC0) — quaternius.com
- **Animals:** Quaternius animated farm animals and animal packs (CC0) — opengameart.org (uploader: quaternius)

## Audio
- **Freesound** — freesound.org — filter to CC0 (sounds are individually licensed; avoid NC).
- **Kenney audio packs** — CC0.
- **OpenGameArt** — filter to CC0.

## License watch-outs
- itch.io "name your price" packs: the free tier is CC0; paid tiers add extra models.
- Some free packs are **CC-BY** (credit required), e.g. Kyrise's Voxel Mines pack is CC BY 4.0.
- Keep a `CREDITS.md` from day one listing every asset, source and license.
