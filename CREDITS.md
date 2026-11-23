# Credits

| Asset | Author | License | Where it is | Source |
|---|---|---|---|---|
| Stylized Nature MegaKit (Standard): trees, bushes, rocks, flowers, ferns, grass, clover, mushrooms, pebbles | Quaternius | CC0 1.0 (public domain) | `game/assets/models`, `game/assets/textures` | https://quaternius.itch.io/stylized-nature-megakit · https://opengameart.org/content/stylized-nature-megakit |
| Medieval Village MegaKit (Standard): walls, windows, doors, gable ends, chimneys, tower roofs, roof-tile texture, wagon and crate, built into the bridge city's houses and towers | Quaternius | CC0 1.0 (public domain) | `game/assets/models/House_*`, `Tower_*`, `Prop_Wagon`, `Prop_Crate`; `game/assets/textures/T_*` | https://quaternius.itch.io/medieval-village-megakit · https://opengameart.org/content/medieval-village-megakit |

The models were trimmed for the web with `tools/prepare_megakit.py`: their
material blocks are removed (the game makes its own materials), vertex-colour
masks on small plants are softened, textures are shrunk to 256–512 px, and a
green copy of the red twisted-tree leaves was made
(`Leaves_TwistedTree_Green.png`). CC0 needs no credit; this file keeps track
of where everything came from.

The bridge city's houses and towers were assembled from the Medieval Village
MegaKit's pieces with `tools/prepare_village.py`: each house is the kit's
2 m walls, corner posts, windows, door and gable ends snapped round a
footprint, with a simple pitched roof using the kit's round-tile texture
(the kit's own tiled roofs are too detailed for a whole city), saved as one
model with its parts grouped by material. Textures are shrunk to 256–512 px.
