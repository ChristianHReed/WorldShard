"""Builds the bridge city's buildings from the Quaternius Medieval Village
MegaKit (Standard, CC0): snaps the kit's 2 m wall pieces, corner posts,
windows, doors, tiled roofs, gable ends and chimneys into whole houses and
towers, and writes each as one glTF model whose parts are grouped by
material (the game makes its own materials). Also shrinks the kit's
textures to web sizes.

Usage: python3 tools/prepare_village.py <kit glTF folder> <game/assets>

The house list must match `HOUSE_TYPES` in src/city.rs.
"""
import json, math, os, random, struct, sys
import numpy as np
from PIL import Image

SRC, DST = sys.argv[1], sys.argv[2]

# parts, in this order, in every building (the game's MODELS table relies on it)
PARTS = ["MI_WoodTrim", "MI_Plaster", "MI_UnevenBrick", "MI_RoundTiles",
         "MI_WindowGlass", "MI_RockTrim", "MI_Brick", "MI_MetalOrnaments"]
SAME = {"MI_WoodTrim_Wear": "MI_WoodTrim", "MI_RedBrick": "MI_Brick"}

# name, width (x, m), depth (z, m), storeys
HOUSES = [
    ("House_4x6_2", 4, 6, 2),
    ("House_4x8_3", 4, 8, 3),
    ("House_6x6_2", 6, 6, 2),
    ("House_6x8_2", 6, 8, 2),
    ("House_6x8_3", 6, 8, 3),
    ("House_6x10_2", 6, 10, 2),
    ("House_8x8_3", 8, 8, 3),
    ("House_8x12_2", 8, 12, 2),
]
TOWERS = [("Tower_4x4_6", 6), ("Tower_4x4_9", 9)]
SINGLES = ["Prop_Wagon", "Prop_Crate"]
TEXTURES = ["T_WoodTrim_BaseColor", "T_Plaster_BaseColor", "T_UnevenBrick_BaseColor",
            "T_RoundTiles_BaseColor", "T_RockTrim_BaseColor", "T_Brick_BaseColor",
            "T_MetalOrnaments_BaseColor"]

_cache = {}


def module(name):
    """A kit piece: list of (material, positions, normals, uvs, indices)."""
    if name in _cache:
        return _cache[name]
    g = json.load(open(f"{SRC}/{name}.gltf"))
    buf = open(f"{SRC}/{g['buffers'][0]['uri']}", "rb").read()

    def acc(i):
        a = g["accessors"][i]
        v = g["bufferViews"][a["bufferView"]]
        comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        dt = {5126: np.float32, 5123: np.uint16, 5125: np.uint32, 5121: np.uint8}[a["componentType"]]
        off = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        n = a["count"] * comps
        stride = v.get("byteStride")
        isz = np.dtype(dt).itemsize
        if stride and stride != comps * isz:
            raw = np.frombuffer(buf, dtype=np.uint8, count=stride * (a["count"] - 1) + comps * isz, offset=off)
            out = np.stack([np.frombuffer(raw[k * stride:k * stride + comps * isz].tobytes(), dtype=dt) for k in range(a["count"])])
            return out.reshape(a["count"], comps) if comps > 1 else out.reshape(-1)
        arr = np.frombuffer(buf, dtype=dt, count=n, offset=off)
        return arr.reshape(-1, comps) if comps > 1 else arr

    mats = [m["name"] for m in g.get("materials", [])]
    prims = []
    for p in g["meshes"][0]["primitives"]:
        mat = mats[p["material"]] if "material" in p else "MI_WoodTrim"
        mat = SAME.get(mat, mat)
        if mat not in PARTS:
            continue
        pos = acc(p["attributes"]["POSITION"]).astype(np.float32)
        nrm = acc(p["attributes"]["NORMAL"]).astype(np.float32)
        uv = acc(p["attributes"]["TEXCOORD_0"]).astype(np.float32) if "TEXCOORD_0" in p["attributes"] else np.zeros((len(pos), 2), np.float32)
        idx = acc(p["indices"]).astype(np.uint32) if "indices" in p else np.arange(len(pos), dtype=np.uint32)
        prims.append((mat, pos, nrm, uv, idx))
    _cache[name] = prims
    return prims


class Builder:
    def __init__(self):
        self.parts = {m: [[], [], [], []] for m in PARTS}
        self.counts = {m: 0 for m in PARTS}

    def add(self, name, x=0.0, y=0.0, z=0.0, rot=0.0):
        """Place a kit piece at (x, y, z), turned `rot` degrees about the up axis
        (90 turns the piece's outward +z side to +x)."""
        c, s = math.cos(math.radians(rot)), math.sin(math.radians(rot))
        r = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]], np.float32)
        for mat, pos, nrm, uv, idx in module(name):
            p = pos @ r.T + np.array([x, y, z], np.float32)
            n = nrm @ r.T
            part = self.parts[mat]
            part[0].append(p)
            part[1].append(n)
            part[2].append(uv)
            part[3].append(idx + self.counts[mat])
            self.counts[mat] += len(pos)

    def add_raw(self, mat, pos, nrm, uv, idx):
        """Add hand-made geometry (positions, normals, uvs, triangle indices)."""
        part = self.parts[mat]
        part[0].append(np.asarray(pos, np.float32))
        part[1].append(np.asarray(nrm, np.float32))
        part[2].append(np.asarray(uv, np.float32))
        part[3].append(np.asarray(idx, np.uint32) + self.counts[mat])
        self.counts[mat] += len(pos)

    def roof(self, w, l, top):
        """A simple pitched roof of the kit's round tiles over a w x l
        footprint, its ridge along z, matching the kit's gable ends. (The
        kit's own tiled roofs are modelled tile by tile: too heavy for a
        whole city.)"""
        peak = {4: 2.94, 6: 4.38, 8: 5.31}[w]
        ox, oz = 0.55, 0.45
        ex = w / 2 + ox
        ey = top - ox * peak / (w / 2)
        ry = top + peak + 0.12
        z0, z1 = -l / 2 - oz, l / 2 + oz
        slope = math.hypot(ex, ry - ey)
        for sgn in (1, -1):
            nx, ny = sgn * (ry - ey), ex
            nl = math.hypot(nx, ny)
            n = [nx / nl, ny / nl, 0.0]
            pos = [[sgn * ex, ey, z0], [sgn * ex, ey, z1], [0, ry, z1], [0, ry, z0]]
            # the tile texture repeats every 2 m
            uv = [[z0 / 2, slope / 2], [z1 / 2, slope / 2], [z1 / 2, 0], [z0 / 2, 0]]
            idx = [0, 1, 2, 0, 2, 3] if sgn < 0 else [0, 2, 1, 0, 3, 2]
            self.add_raw("MI_RoundTiles", pos, [n] * 4, uv, idx)
        # a ridge board along the top
        rp = [[-0.18, ry - 0.05, z0], [0.18, ry - 0.05, z0], [0.18, ry - 0.05, z1], [-0.18, ry - 0.05, z1],
              [-0.1, ry + 0.12, z0], [0.1, ry + 0.12, z0], [0.1, ry + 0.12, z1], [-0.1, ry + 0.12, z1]]
        self.add_raw("MI_WoodTrim", rp, [[0, 1, 0]] * 8, [[0, 0], [0.1, 0], [0.1, 1], [0, 1]] * 2,
                     [4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 3, 2, 6, 3, 6, 7])

    def write(self, name):
        blob = bytearray()
        views, accs, prims = [], [], []

        def put(arr, target, kind, comp, minmax=False):
            while len(blob) % 4:
                blob.append(0)
            off = len(blob)
            data = arr.tobytes()
            blob.extend(data)
            views.append({"buffer": 0, "byteOffset": off, "byteLength": len(data), "target": target})
            a = {"bufferView": len(views) - 1, "componentType": comp, "count": int(arr.shape[0]) if arr.ndim > 1 else int(arr.size), "type": kind}
            if minmax:
                a["min"] = [float(v) for v in arr.min(axis=0)]
                a["max"] = [float(v) for v in arr.max(axis=0)]
            accs.append(a)
            return len(accs) - 1

        top = 0.0
        for m in PARTS:
            p, n, uv, idx = self.parts[m]
            if p:
                P, N, U, I = (np.concatenate(p), np.concatenate(n), np.concatenate(uv), np.concatenate(idx))
                top = max(top, float(P[:, 1].max()))
            else:
                # an empty part still needs a (degenerate) triangle
                P = np.zeros((3, 3), np.float32)
                N = np.tile(np.array([0, 1, 0], np.float32), (3, 1))
                U = np.zeros((3, 2), np.float32)
                I = np.array([0, 1, 2], np.uint32)
            if I.max() < 65535:
                ii = put(I.astype(np.uint16), 34963, "SCALAR", 5123)
            else:
                ii = put(I.astype(np.uint32), 34963, "SCALAR", 5125)
            prims.append({
                "attributes": {
                    "POSITION": put(P.astype(np.float32), 34962, "VEC3", 5126, True),
                    "NORMAL": put(N.astype(np.float32), 34962, "VEC3", 5126),
                    "TEXCOORD_0": put(U.astype(np.float32), 34962, "VEC2", 5126),
                },
                "indices": ii,
            })
        g = {
            "asset": {"version": "2.0", "generator": "WorldShard prepare_village.py"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0, "name": name}],
            "meshes": [{"name": name, "primitives": prims}],
            "buffers": [{"uri": f"{name}.bin", "byteLength": len(blob)}],
            "bufferViews": views,
            "accessors": accs,
        }
        os.makedirs(f"{DST}/models", exist_ok=True)
        json.dump(g, open(f"{DST}/models/{name}.gltf", "w"), separators=(",", ":"))
        open(f"{DST}/models/{name}.bin", "wb").write(bytes(blob))
        return top


def walls_around(b, w, l, y, ground, rnd, door=True):
    """One storey of walls round a w x l footprint (centred), base at y."""
    sides = [
        # (outward rotation, positions of module centres along the side)
        (0.0, [(-w / 2 + 1 + 2 * i, l / 2) for i in range(w // 2)]),
        (180.0, [(-w / 2 + 1 + 2 * i, -l / 2) for i in range(w // 2)]),
        (90.0, [(w / 2, -l / 2 + 1 + 2 * i) for i in range(l // 2)]),
        (-90.0, [(-w / 2, -l / 2 + 1 + 2 * i) for i in range(l // 2)]),
    ]
    for si, (rot, spots) in enumerate(sides):
        for k, (x, z) in enumerate(spots):
            front_door = door and ground and si == 0 and k == len(spots) // 2
            if si >= 2:
                # side walls face the neighbours across a narrow gap: plain
                b.add("Wall_UnevenBrick_Straight" if ground else "Wall_Plaster_Straight", x, y, z, rot)
            elif ground:
                if front_door:
                    b.add("Wall_UnevenBrick_Door_Flat", x, y, z, rot)
                    # the door leaf, centred in its opening
                    c, s = math.cos(math.radians(rot)), math.sin(math.radians(rot))
                    dx, dz = -0.53, -0.11
                    b.add("Door_1_Flat", x + dx * c + dz * s, y, z - dx * s + dz * c, rot)
                elif rnd.random() < 0.3:
                    b.add("Wall_UnevenBrick_Window_Wide_Round", x, y, z, rot)
                    b.add("Window_Wide_Round1", x, y, z, rot)
                else:
                    b.add("Wall_UnevenBrick_Straight", x, y, z, rot)
            else:
                r = rnd.random()
                if r < 0.3:
                    b.add("Wall_Plaster_Window_Wide_Flat", x, y, z, rot)
                    b.add("Window_Wide_Flat1", x, y, z, rot)
                elif r < 0.45:
                    b.add("Wall_Plaster_WoodGrid", x, y, z, rot)
                else:
                    b.add("Wall_Plaster_Straight", x, y, z, rot)
    for x in (-w / 2, w / 2):
        for z in (-l / 2, l / 2):
            b.add("Corner_Exterior_Wood", x, y, z, 0.0)


def house(name, w, l, storeys, seed):
    rnd = random.Random(seed)
    b = Builder()
    for k in range(storeys):
        walls_around(b, w, l, 3.0 * k, k == 0, rnd)
    top = 3.0 * storeys
    b.roof(w, l, top)
    b.add(f"Roof_Front_Brick{w}", 0, top, l / 2, 0)
    b.add(f"Roof_Front_Brick{w}", 0, top, -l / 2, 180)
    if rnd.random() < 0.8:
        b.add("Prop_Chimney2", w / 4, top + 0.4, rnd.choice([-1, 1]) * l / 4, 0)
    return b.write(name)


def tower(name, storeys, seed):
    rnd = random.Random(seed)
    b = Builder()
    for k in range(storeys):
        ground = k == 0
        for rot, spots in [(0.0, [(-1, 2), (1, 2)]), (180.0, [(-1, -2), (1, -2)]), (90.0, [(2, -1), (2, 1)]), (-90.0, [(-2, -1), (-2, 1)])]:
            for i, (x, z) in enumerate(spots):
                if ground and rot == 0.0 and i == 0:
                    b.add("Wall_UnevenBrick_Door_Round", x, 0, z, rot)
                elif not ground and rnd.random() < 0.12:
                    b.add("Wall_UnevenBrick_Window_Thin_Round", x, 3.0 * k, z, rot)
                    b.add("Window_Thin_Round1", x, 3.0 * k, z, rot)
                else:
                    b.add("Wall_UnevenBrick_Straight", x, 3.0 * k, z, rot)
        for x in (-2, 2):
            for z in (-2, 2):
                b.add("Corner_Exterior_Wood", x, 3.0 * k, z, 0.0)
    b.add("Roof_Tower_RoundTiles", 0, 3.0 * storeys, 0, 0)
    return b.write(name)


def single(name):
    b = Builder()
    b.add(name)
    return b.write(name)


manifest = {}
for i, (name, w, l, st) in enumerate(HOUSES):
    manifest[name] = round(house(name, w, l, st, 100 + i), 2)
for i, (name, st) in enumerate(TOWERS):
    manifest[name] = round(tower(name, st, 200 + i), 2)
for name in SINGLES:
    manifest[name] = round(single(name), 2)

os.makedirs(f"{DST}/textures", exist_ok=True)
for t in TEXTURES:
    im = Image.open(f"{SRC}/{t}.png").convert("RGB")
    size = 256 if "Metal" in t else 512
    im = im.resize((size, size), Image.LANCZOS)
    im.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(f"{DST}/textures/{t}.png", optimize=True)

print(json.dumps(manifest, indent=1))
