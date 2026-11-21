"""Prepares the Quaternius Stylized Nature MegaKit (Standard, CC0) for the
web game: copies the chosen glTF models without their material blocks (the
game makes its own materials), softens the vertex-colour masks on small
plants, and shrinks the textures to web sizes.

Usage: python3 tools/prepare_megakit.py <MegaKit glTF folder> <game/assets>
"""
import json, os, shutil, sys
import numpy as np
from PIL import Image

SRC, DST = sys.argv[1], sys.argv[2]
MODELS = (
    [f"CommonTree_{i}" for i in range(1, 6)]
    + [f"Pine_{i}" for i in range(1, 6)]
    + ["TwistedTree_2", "TwistedTree_5"]
    + ["DeadTree_1", "DeadTree_3", "DeadTree_5"]
    + ["Bush_Common", "Bush_Common_Flowers"]
    + [f"Rock_Medium_{i}" for i in range(1, 4)]
    + ["Fern_1", "Plant_1_Big", "Plant_7_Big"]
    + ["Flower_3_Group", "Flower_4_Group", "Flower_3_Single", "Flower_4_Single"]
    + ["Grass_Common_Short", "Grass_Common_Tall", "Grass_Wispy_Short", "Grass_Wispy_Tall"]
    + ["Clover_1", "Clover_2", "Mushroom_Common", "Mushroom_Laetiporus"]
    + ["Pebble_Round_1", "Pebble_Round_2", "Pebble_Round_3", "Pebble_Square_1", "Pebble_Square_2"]
)
# small plants: their vertex colours are shading masks; soften them
SOFTEN = ("Grass", "Fern", "Clover", "Flower", "Plant")
TEXTURES = {
    "Bark_NormalTree.png": 512, "Bark_DeadTree.png": 512, "Bark_TwistedTree.png": 512,
    "Leaves_NormalTree_C.png": 512, "Leaf_Pine_C.png": 512, "Leaves_TwistedTree_C.png": 512,
    "Leaves.png": 512, "Flowers.png": 512, "Grass.png": 256, "Mushrooms.png": 256,
    "PathRocks_Diffuse.png": 256, "Rocks_Diffuse.png": 512, "Rocks_Desert_Diffuse.png": 512,
}

os.makedirs(f"{DST}/models", exist_ok=True)
os.makedirs(f"{DST}/textures", exist_ok=True)
manifest = {}
for name in MODELS:
    g = json.load(open(f"{SRC}/{name}.gltf"))
    mats = [m["name"] for m in g.get("materials", [])]
    parts = []
    buf = bytearray(open(f"{SRC}/{g['buffers'][0]['uri']}", "rb").read())
    for p in g["meshes"][0]["primitives"]:
        parts.append(mats[p["material"]] if "material" in p else None)
        p.pop("material", None)
        if "COLOR_0" in p["attributes"] and name.startswith(SOFTEN):
            a = g["accessors"][p["attributes"]["COLOR_0"]]
            bv = g["bufferViews"][a["bufferView"]]
            off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
            n = {"VEC3": 3, "VEC4": 4}[a["type"]]
            dt, scale = {5126: (np.float32, 1.0), 5123: (np.uint16, 65535.0), 5121: (np.uint8, 255.0)}[a["componentType"]]
            arr = np.frombuffer(bytes(buf[off: off + a["count"] * n * np.dtype(dt).itemsize]), dtype=dt).reshape(-1, n).astype(np.float64) / scale
            arr[:, :3] = 0.55 + 0.45 * arr[:, :3]
            out = (arr * scale).round().astype(dt) if dt != np.float32 else arr.astype(np.float32)
            buf[off: off + out.nbytes] = out.tobytes()
    for k in ("materials", "textures", "images", "samplers"):
        g.pop(k, None)
    bin_name = f"{name}.bin"
    g["buffers"][0]["uri"] = bin_name
    json.dump(g, open(f"{DST}/models/{name}.gltf", "w"), separators=(",", ":"))
    open(f"{DST}/models/{bin_name}", "wb").write(buf)
    manifest[name] = parts

for f, size in TEXTURES.items():
    im = Image.open(f"{SRC}/{f}")
    im = im.convert("RGBA" if im.mode == "RGBA" else "RGB")
    im = im.resize((size, size), Image.LANCZOS)
    im.save(f"{DST}/textures/{f}", optimize=True)

# the twisted-tree / bush leaves are autumn red; make a green version
im = Image.open(f"{DST}/textures/Leaves_TwistedTree_C.png").convert("RGBA")
a = im.getchannel("A")
h, sat, v = im.convert("RGB").convert("HSV").split()
h = h.point(lambda x: (x + 66) % 256)
green = Image.merge("HSV", (h, sat.point(lambda x: int(x * 0.85)), v)).convert("RGB")
green.putalpha(a)
green.save(f"{DST}/textures/Leaves_TwistedTree_Green.png", optimize=True)

json.dump(manifest, open(f"{DST}/models/parts.json", "w"), indent=1)
shutil.copy(f"{SRC}/../License_Standard.txt", f"{DST}/models/LICENSE-MegaKit.txt")
print("models:", len(manifest))
