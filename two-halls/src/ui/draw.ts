import { SEATS, isNorth } from "../game/content";
import { BUILDINGS, COLS, ROWS } from "../game/content";
import { terrainAt } from "../game/map";
import { canSee, type Building, type Node, type PlayerId, type Sim, type Unit } from "../game/sim";

type G = CanvasRenderingContext2D;
const TEAM = SEATS;

const rnd = (x: number, y: number, k: number) => {
  const v = Math.sin(x * 127.1 + y * 311.7 + k * 74.7) * 43758.5453;
  return v - Math.floor(v);
};

function box(g: G, x: number, y: number, w: number, h: number, c: string) {
  g.fillStyle = c;
  g.fillRect(x, y, w, h);
}
function disc(g: G, x: number, y: number, r: number, c: string) {
  g.fillStyle = c;
  g.beginPath();
  g.arc(x, y, r, 0, Math.PI * 2);
  g.fill();
}
function poly(g: G, c: string, ...p: number[]) {
  g.fillStyle = c;
  g.beginPath();
  g.moveTo(p[0]!, p[1]!);
  for (let i = 2; i < p.length; i += 2) g.lineTo(p[i]!, p[i + 1]!);
  g.closePath();
  g.fill();
}
function shadow(g: G, x: number, y: number, rx: number, ry: number) {
  g.fillStyle = "rgba(0,0,0,.25)";
  g.beginPath();
  g.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2);
  g.fill();
}

function drawTile(g: G, tx: number, ty: number, x: number, y: number) {
  const t = terrainAt(tx, ty);
  const base = t === "water" ? "#2c6f9e" : t === "dirt" ? "#a8864f" : (tx + ty) % 2 ? "#628c3c" : "#668f3f";
  box(g, x, y, 33, 33, base);
  for (let i = 0; i < 5; i++) {
    const px = x + rnd(tx, ty, i) * 28;
    const py = y + rnd(ty, tx, i + 9) * 28;
    if (t === "water") box(g, px, py, 7, 1.5, "rgba(190,230,255,.35)");
    else if (t === "dirt") box(g, px, py, 3, 2, i % 2 ? "#8f6f3f" : "#bd9c64");
    else box(g, px, py, 2, 4, i % 2 ? "#537a31" : "#7aa54c");
  }
}

function drawFish(g: G, n: Node, x: number, y: number, clock: number) {
  const bob = Math.sin(clock * 2.2 + n.id) * 1.5;
  g.strokeStyle = "rgba(210,240,255,.55)";
  g.lineWidth = 1.5;
  g.beginPath();
  g.ellipse(x + 16, y + 17, 13 + bob, 8 + bob / 2, 0, 0, Math.PI * 2);
  g.stroke();
  for (const [dx, dy, f] of [[9, 12, 1], [22, 18, -1], [13, 24, 1]] as const) {
    const fy = y + dy + bob * f;
    g.fillStyle = "#f4c9a8";
    g.beginPath();
    g.ellipse(x + dx, fy, 5.5, 2.8, 0, 0, Math.PI * 2);
    g.fill();
    poly(g, "#e89a78", x + dx - f * 4, fy, x + dx - f * 9, fy - 3.5, x + dx - f * 9, fy + 3.5);
    box(g, x + dx + f * 2, fy - 1, 1.5, 1.5, "#23323a");
  }
}

function drawShip(g: G, u: Unit, x: number, y: number, face: number, clock: number) {
  const c = TEAM[u.owner];
  const rock = Math.sin(clock * 2 + u.id) * 1.2;
  y += rock;
  const big = u.kind === "fisher" ? 0.78 : u.kind === "bombard" ? 1.12 : 1;
  const w = 21 * big;
  g.fillStyle = "rgba(220,245,255,.4)";
  g.beginPath();
  g.ellipse(x, y + 8, w + 3, 5, 0, 0, Math.PI * 2);
  g.fill();
  if (u.kind === "galley") {
    const stroke = u.path.length ? Math.sin(clock * 9 + u.id) * 4 : 0;
    for (const dx of [-11, -3, 5, 13]) box(g, x + dx + stroke, y + 4, 2, 11, "#caa56a");
  }
  poly(g, u.flash > 0 ? "#fff" : u.kind === "bombard" ? "#4a2f18" : "#7a4d22", x - w, y - 2, x + w, y - 2, x + w * 0.65, y + 9, x - w * 0.65, y + 9);
  box(g, x - w, y - 4, w * 2, 3, "#b98a52");
  box(g, x - w * 0.8, y + 1, w * 1.6, 3, c.main);
  box(g, x - 1, y - 28 * big, 2.5, 26 * big, "#3b2514");
  if (u.kind === "fisher") {
    poly(g, c.light, x + face * 2, y - 21, x + face * 14, y - 8, x + face * 2, y - 8);
    g.strokeStyle = "#e8e0c8";
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(x - face * 6, y - 4);
    g.lineTo(x - face * 20, y - 13);
    g.lineTo(x - face * 22, y + 8);
    g.stroke();
    if (u.carry >= 1) disc(g, x - face * 7, y - 5, 3.5, "#f4c9a8");
  } else {
    poly(g, c.main, x - 12 * big, y - 25 * big, x + 12 * big, y - 25 * big, x + 15 * big, y - 9, x - 15 * big, y - 9);
    box(g, x - 12 * big, y - 18 * big, 24 * big, 3, c.light);
    poly(g, c.light, x, y - 28 * big, x - face * 12, y - 32 * big, x, y - 34 * big);
    if (u.kind === "bombard") {
      box(g, x + face * 10 - 7, y - 9, 14, 6, "#22262a");
      disc(g, x + face * 18, y - 6, 3.5, "#22262a");
    } else poly(g, "#e8eef0", x + face * w, y - 2, x + face * (w + 8), y - 1, x + face * w, y + 2);
  }
}

function drawNode(g: G, n: Node, x: number, y: number) {
  const cx = x + 16;
  if (n.kind === "wood") {
    shadow(g, cx, y + 27, 13, 5);
    box(g, cx - 3, y + 18, 6, 10, "#6b431f");
    poly(g, "#1f4a24", cx - 14, y + 22, cx, y - 2, cx + 14, y + 22);
    poly(g, "#2d6a31", cx - 11, y + 13, cx, y - 9, cx + 11, y + 13);
    poly(g, "#3f8540", cx - 5, y + 3, cx, y - 9, cx + 3, y + 3);
  } else if (n.kind === "food") {
    shadow(g, cx, y + 26, 13, 5);
    for (const [dx, dy] of [[-6, 4], [6, 4], [0, -3]] as const) disc(g, cx + dx, y + 16 + dy, 8, "#35702f");
    for (const [dx, dy] of [[-8, 2], [-2, 8], [3, -5], [8, 5], [1, 2], [-3, -4]] as const)
      disc(g, cx + dx, y + 16 + dy, 2.4, "#c92f52");
  } else {
    const gold = n.kind === "gold";
    shadow(g, cx, y + 27, 14, 5);
    poly(g, gold ? "#7d6a45" : "#71777a", x + 2, y + 27, x + 8, y + 10, x + 18, y + 6, x + 29, y + 18, x + 30, y + 27);
    poly(g, gold ? "#a38b58" : "#a3a9a8", x + 8, y + 10, x + 18, y + 6, x + 19, y + 18, x + 9, y + 22);
    if (gold)
      for (const [dx, dy] of [[11, 15], [20, 12], [23, 21], [14, 23]] as const) {
        poly(g, "#ffd23f", x + dx, y + dy - 4, x + dx + 4, y + dy, x + dx, y + dy + 3, x + dx - 3, y + dy);
      }
  }
}

/** links: which sides (screen up, right, down, left) join another wall */
function drawWall(g: G, b: Building, x: number, y: number, links: boolean[]) {
  const c = TEAM[b.owner];
  const stone = b.kind === "wall";
  const body = stone ? "#9aa09a" : b.kind === "gate" ? "#6e4522" : "#9a6a35";
  const top = stone ? "#c2c7bd" : "#c39254";
  const arms = [[8, 0, 16, 16], [16, 8, 16, 16], [8, 16, 16, 16], [0, 8, 16, 16]] as const;
  arms.forEach(([ax, ay, aw, ah], i) => {
    if (!links[i]) return;
    box(g, x + ax, y + ay, aw, ah, body);
    box(g, x + ax + (aw > ah || i % 2 ? 0 : 3), y + ay + (i % 2 ? 3 : 0), i % 2 ? aw : 10, i % 2 ? 8 : ah, top);
  });
  if (b.kind === "gate") {
    const sideways = (links[1] || links[3]) && !(links[0] || links[2]);
    box(g, x + 3, y + 3, 26, 26, "#3c2512");
    for (let i = 0; i < 4; i++) {
      if (sideways) box(g, x + 6 + i * 5.5, y + 6, 4, 20, body);
      else box(g, x + 6, y + 6 + i * 5.5, 20, 4, body);
    }
    box(g, x + 2, y + 2, 6, 6, c.main);
    box(g, x + 24, y + 2, 6, 6, c.main);
    box(g, x + 2, y + 24, 6, 6, c.main);
    box(g, x + 24, y + 24, 6, 6, c.main);
    return;
  }
  box(g, x + 5, y + 5, 22, 22, "#2a2018");
  box(g, x + 6, y + 6, 20, 20, body);
  box(g, x + 8, y + 7, 16, 11, top);
  box(g, x + 14, y + 20, 4, 3, c.main);
}

function drawBuilding(g: G, b: Building, x: number, y: number, links: boolean[] = []) {
  const s = BUILDINGS[b.kind];
  const w = s.w * 32;
  const h = s.h * 32;
  const c = TEAM[b.owner];
  if (b.progress < 1) {
    box(g, x + 3, y + 3, w - 6, h - 6, "rgba(90,60,30,.55)");
    g.strokeStyle = "#e0c07a";
    g.lineWidth = 2;
    g.strokeRect(x + 4, y + 4, w - 8, h - 8);
    const rise = (h - 14) * b.progress;
    box(g, x + 9, y + h - 7 - rise, w - 18, rise, "#a57b4a");
    for (let px = x + 8; px < x + w - 8; px += 14) box(g, px, y + 8, 3, h - 15, "#6e4a26");
    return;
  }
  if (BUILDINGS[b.kind].wall) {
    drawWall(g, b, x, y, links);
    return;
  }
  if (b.kind === "farm") {
    box(g, x + 2, y + 2, w - 4, h - 4, "#8a6a35");
    for (let i = 0; i < 6; i++) box(g, x + 5, y + 6 + i * 9.5, w - 10, 4, "#e0bd4a");
    g.strokeStyle = c.main;
    g.lineWidth = 3;
    g.strokeRect(x + 2.5, y + 2.5, w - 5, h - 5);
    return;
  }
  if (b.kind === "dock") {
    box(g, x + 2, y + 6, w - 4, h - 8, "#5a3a1c");
    for (let i = 0; i < 7; i++) box(g, x + 3, y + 8 + i * 8, w - 6, 6, i % 2 ? "#a5773f" : "#b98a52");
    for (const dx of [2, w - 8]) for (const dy of [6, h - 10]) disc(g, x + dx + 3, y + dy + 3, 4.5, "#3c2512");
    box(g, x + 14, y + 16, 30, 24, "#cbb894");
    poly(g, c.main, x + 10, y + 19, x + 29, y + 2, x + 48, y + 19);
    box(g, x + 24, y + 27, 10, 13, "#4a2c17");
    box(g, x + w - 16, y - 4, 3, 30, "#4a2c17");
    poly(g, c.light, x + w - 13, y - 4, x + w + 3, y + 1, x + w - 13, y + 7);
    return;
  }
  shadow(g, x + w / 2, y + h - 5, w / 2, 7);
  if (b.kind === "tower") {
    box(g, x + 15, y + 12, w - 30, h - 18, "#8d928c");
    box(g, x + 17, y + 12, 9, h - 18, "#b4b8ac");
    box(g, x + 11, y + 8, w - 22, 12, "#9ea39b");
    for (let i = 0; i < 4; i++) box(g, x + 11 + i * 12, y + 2, 7, 8, "#9ea39b");
    poly(g, c.main, x + 18, y + 3, x + 32, y - 18, x + 46, y + 3);
    box(g, x + 28, y + h - 22, 9, 16, "#3c2414");
    box(g, x + 29, y + 28, 6, 9, "#25343a");
    return;
  }
  const wall = b.kind === "town" ? "#cbb894" : b.kind === "barracks" ? "#9a8068" : b.kind === "smithy" ? "#7d7a76" : b.kind === "workshop" ? "#8f6f4a" : "#c29a68";
  const roofH = h * 0.5;
  box(g, x + 7, y + roofH - 2, w - 14, h - roofH - 4, wall);
  box(g, x + 7, y + h - 12, w - 14, 6, "rgba(0,0,0,.18)");
  poly(g, c.main, x + 2, y + roofH, x + w * 0.22, y + 6, x + w * 0.78, y + 6, x + w - 2, y + roofH);
  poly(g, c.dark, x + 2, y + roofH, x + w - 2, y + roofH, x + w - 5, y + roofH + 6, x + 5, y + roofH + 6);
  box(g, x + w * 0.22, y + 6, w * 0.56, 4, c.light);
  const doorW = b.kind === "town" ? 20 : 12;
  box(g, x + w / 2 - doorW / 2, y + h - 24, doorW, 18, "#4a2c17");
  if (b.kind === "town") {
    for (const dx of [18, w - 30]) box(g, x + dx, y + h - 30, 12, 12, "#31505a");
    box(g, x + w / 2 - 1, y - 20, 3, 28, "#4a2c17");
    poly(g, c.light, x + w / 2 + 2, y - 20, x + w / 2 + 22, y - 14, x + w / 2 + 2, y - 8);
  } else if (b.kind === "house") {
    box(g, x + w - 20, y + 2, 8, 14, "#7a706a");
  } else if (b.kind === "barracks") {
    g.strokeStyle = "#e8eef0";
    g.lineWidth = 3;
    g.beginPath();
    g.moveTo(x + 16, y + h - 10);
    g.lineTo(x + 30, y + h - 28);
    g.moveTo(x + 30, y + h - 10);
    g.lineTo(x + 16, y + h - 28);
    g.stroke();
  } else if (b.kind === "store") {
    for (const [dx, dy] of [[9, h - 20], [20, h - 17], [w - 24, h - 20]] as const) {
      box(g, x + dx, y + dy, 13, 13, "#8a5a2b");
      box(g, x + dx + 2, y + dy + 5, 9, 2, "#d9a566");
    }
  } else if (b.kind === "stable") {
    disc(g, x + 22, y + h - 18, 9, "#4a2c17");
    box(g, x + w - 34, y + h - 20, 24, 12, "#e0bd4a");
  } else if (b.kind === "workshop") {
    for (const dx of [20, w - 20]) {
      disc(g, x + dx, y + h - 16, 10, "#4a2c17");
      disc(g, x + dx, y + h - 16, 6, "#c39254");
      disc(g, x + dx, y + h - 16, 2, "#4a2c17");
    }
  } else if (b.kind === "smithy") {
    box(g, x + w - 20, y, 10, 18, "#4d4a47");
    disc(g, x + w - 15, y - 3, 4, "rgba(210,210,210,.6)");
    box(g, x + 11, y + h - 19, 14, 5, "#23262a");
    box(g, x + 15, y + h - 14, 6, 7, "#23262a");
    box(g, x + w - 22, y + h - 19, 9, 9, "#ff8a24");
  } else if (b.kind === "range") {
    disc(g, x + w - 22, y + h - 19, 11, "#efe6cf");
    disc(g, x + w - 22, y + h - 19, 7, "#b83a2c");
    disc(g, x + w - 22, y + h - 19, 3, "#efe6cf");
  }
}

function drawUnit(g: G, u: Unit, x: number, y: number, face: number, clock: number) {
  if (u.kind === "fisher" || u.kind === "galley" || u.kind === "bombard") {
    drawShip(g, u, x, y, face, clock);
    return;
  }
  const c = TEAM[u.owner];
  const walking = u.order.t === "move" || u.path.length > 0;
  const bob = walking ? Math.sin(u.anim * 1.6) * 1.5 : 0;
  const working = (u.order.t === "gather" || u.order.t === "build") && !u.path.length;
  const swing = working ? Math.sin(clock * 11 + u.id) * 3 : 0;
  if (u.kind === "ram") {
    shadow(g, x, y + 10, 19, 6);
    box(g, x - 15, y - 8, 30, 15, "#6e4522");
    poly(g, u.flash > 0 ? "#fff" : c.main, x - 17, y - 7, x, y - 19, x + 17, y - 7);
    box(g, x + face * 8 - 11, y - 3, 22, 6, "#b98a52");
    box(g, x + face * 20 - 3, y - 4, 6, 8, "#7c848a");
    for (const dx of [-10, 10]) {
      disc(g, x + dx, y + 8, 5.5, "#2a1a0d");
      disc(g, x + dx, y + 8, 2, "#c39254");
    }
    return;
  }
  if (u.kind === "rider") {
    shadow(g, x, y + 11, 14, 5);
    for (const dx of [-9, -4, 4, 9]) box(g, x + dx - 1.5, y + 2, 3, 9 + (Math.abs(dx) > 5 ? bob : -bob), "#3b2514");
    g.fillStyle = "#7a4d28";
    g.beginPath();
    g.ellipse(x, y + 1, 13, 7, 0, 0, Math.PI * 2);
    g.fill();
    box(g, x + face * 10 - 3, y - 11, 6, 12, "#7a4d28");
    box(g, x + face * 13 - 4, y - 13, 9, 6, "#8d5b31");
    poly(g, u.flash > 0 ? "#fff" : c.main, x - 6, y, x - 4, y - 13, x + 4, y - 13, x + 6, y);
    disc(g, x, y - 17, 5, "#efc097");
    disc(g, x, y - 19, 5, "#aeb8bd");
    box(g, x - face * 7 - 1, y - 28, 2, 28, "#e8eef0");
    poly(g, c.light, x - face * 7, y - 28, x - face * 18, y - 25, x - face * 7, y - 21);
    return;
  }
  shadow(g, x, y + 9, 8, 3.5);
  box(g, x - 5, y + 3, 4, 7 + bob, "#3b2a1c");
  box(g, x + 1, y + 3, 4, 7 - bob, "#3b2a1c");
  poly(g, "#17130f", x - 9, y + 6, x - 7, y - 7, x + 7, y - 7, x + 9, y + 6);
  poly(g, u.flash > 0 ? "#fff" : c.main, x - 7.5, y + 5, x - 6, y - 6, x + 6, y - 6, x + 7.5, y + 5);
  disc(g, x, y - 11, 6.5, "#17130f");
  disc(g, x, y - 11, 5.2, "#efc097");
  if (u.kind === "villager") {
    box(g, x - 8, y - 17, 16, 3, "#d9b455");
    box(g, x - 4, y - 20, 8, 4, "#d9b455");
    box(g, x + face * 9 - 1, y - 7 + swing, 2.5, 13, "#7a4d22");
    box(g, x + face * 9 - 4, y - 8 + swing, 8, 4, "#cfd6d8");
    if (u.carry >= 1) {
      const col = { food: "#c92f52", wood: "#8a5a2b", gold: "#ffd23f", stone: "#b4b8ac" }[u.carryType ?? "wood"];
      disc(g, x - face * 8, y - 3, 4.5, "#17130f");
      disc(g, x - face * 8, y - 3, 3.5, col);
    }
  } else if (u.kind === "militia") {
    disc(g, x, y - 13, 6, "#aeb8bd");
    box(g, x - 6, y - 13, 12, 3, "#6d777d");
    box(g, x + face * 10 - 1.5, y - 18, 3, 18, "#e8eef0");
    box(g, x + face * 10 - 4, y - 2, 8, 2.5, "#7a4d22");
    disc(g, x - face * 7, y - 1, 5.5, c.light);
    disc(g, x - face * 7, y - 1, 2, c.dark);
  } else {
    poly(g, c.dark, x - 7, y - 10, x, y - 21, x + 7, y - 10);
    g.strokeStyle = "#7a4d22";
    g.lineWidth = 2.5;
    g.beginPath();
    g.arc(x + face * 6, y - 3, 10, face > 0 ? -1.2 : Math.PI - 1.2, face > 0 ? 1.2 : Math.PI + 1.2);
    g.stroke();
  }
}

function hpBar(g: G, x: number, y: number, w: number, f: number) {
  box(g, x - 1, y - 1, w + 2, 5, "#17130f");
  box(g, x, y, w * Math.max(0, f), 3, f > 0.5 ? "#78d64b" : f > 0.25 ? "#f2c230" : "#ee4b3a");
}

/** Draws one player's view. North's view is turned around so their own base is nearest to them. */
export function render(g: G, sim: Sim, p: PlayerId, cssW: number, cssH: number, zoom: number, dpr: number, maxHp: (u: Unit) => number) {
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  box(g, 0, 0, cssW, cssH, "#10140d");
  const dir = isNorth(p) ? -1 : 1;
  const cam = sim.cam[p];
  const shake = sim.shake[p] * 3;
  g.translate(cssW / 2 + (Math.random() - 0.5) * shake, cssH / 2 + (Math.random() - 0.5) * shake);
  g.scale(zoom, zoom);
  const sx = (wx: number) => (wx - cam.x) * 32 * dir;
  const sy = (wy: number) => (wy - cam.y) * 32 * dir;
  /** top-left on screen of a world rectangle */
  const corner = (tx: number, ty: number, w: number, h: number) =>
    [dir > 0 ? sx(tx) : sx(tx + w), dir > 0 ? sy(ty) : sy(ty + h)] as const;

  const halfW = cssW / zoom / 64 + 2;
  const halfH = cssH / zoom / 64 + 2;
  const x0 = Math.max(0, Math.floor(cam.x - halfW));
  const x1 = Math.min(COLS - 1, Math.ceil(cam.x + halfW));
  const y0 = Math.max(0, Math.floor(cam.y - halfH));
  const y1 = Math.min(ROWS - 1, Math.ceil(cam.y + halfH));
  const onScreen = (x: number, y: number, w = 1, h = 1) => x + w >= x0 && x <= x1 + 1 && y + h >= y0 && y <= y1 + 1;

  for (let ty = y0; ty <= y1; ty++)
    for (let tx = x0; tx <= x1; tx++) drawTile(g, tx, ty, ...corner(tx, ty, 1, 1));

  const walls = new Set<number>();
  for (const b of sim.buildings) if (BUILDINGS[b.kind].wall && b.seen & (1 << p)) walls.add((b.ty * COLS + b.tx) * 4 + b.owner);
  const links = (b: Building) =>
    [[0, -1], [1, 0], [0, 1], [-1, 0]].map(([dx, dy]) => walls.has(((b.ty + dy! * dir) * COLS + b.tx + dx! * dir) * 4 + b.owner));

  const items: { z: number; f: () => void }[] = [];
  for (const n of sim.nodes) {
    if (n.farm || n.amount <= 0 || !onScreen(n.x, n.y) || !sim.explored[p][n.y * COLS + n.x]) continue;
    const [x, y] = corner(n.x, n.y, 1, 1);
    if (n.sea) items.push({ z: y, f: () => drawFish(g, n, x, y, sim.clock) });
    else items.push({ z: y + 28, f: () => drawNode(g, n, x, y) });
  }
  const chosen = new Set(sim.sel[p].units);
  for (const b of sim.buildings) {
    if (!(b.seen & (1 << p))) continue;
    const s = BUILDINGS[b.kind];
    if (!onScreen(b.tx, b.ty, s.w, s.h)) continue;
    const [x, y] = corner(b.tx, b.ty, s.w, s.h);
    items.push({
      z: b.kind === "farm" ? y : y + s.h * 32 - 6,
      f: () => {
        drawBuilding(g, b, x, y, s.wall ? links(b) : []);
        const picked = sim.sel[p].building === b.id;
        if (picked) {
          g.strokeStyle = "#fff6a8";
          g.lineWidth = 2;
          g.strokeRect(x, y, s.w * 32, s.h * 32);
        }
        if (picked || b.hp < s.hp - 0.5) hpBar(g, x + 8, y + s.h * 32 - 4, s.w * 32 - 16, b.hp / s.hp);
      },
    });
  }
  for (const u of sim.units) {
    if (!onScreen(u.x, u.y, 0, 0)) continue;
    if (u.owner !== p && !canSee(sim, p, u.x, u.y)) continue;
    const x = sx(u.x);
    const y = sy(u.y);
    items.push({
      z: y + 9,
      f: () => {
        const afloat = u.kind === "fisher" || u.kind === "galley" || u.kind === "bombard";
        if (chosen.has(u.id)) {
          g.strokeStyle = "#fff6a8";
          g.lineWidth = 2;
          g.beginPath();
          g.ellipse(x, y + 9, afloat ? 27 : 12, afloat ? 9 : 6, 0, 0, Math.PI * 2);
          g.stroke();
        }
        drawUnit(g, u, x, y, u.fx * dir < 0 ? -1 : 1, sim.clock);
        if (chosen.has(u.id) || u.hp < maxHp(u) - 0.5) hpBar(g, x - 9, y - (afloat ? 40 : 25), 18, u.hp / maxHp(u));
      },
    });
  }
  items.sort((a, b) => a.z - b.z);
  for (const it of items) it.f();

  for (const a of sim.arrows) {
    if (!canSee(sim, p, a.x, a.y)) continue;
    const ang = Math.atan2((a.ty - a.y) * dir, (a.tx - a.x) * dir);
    g.save();
    g.translate(sx(a.x), sy(a.y) - 6);
    g.rotate(ang);
    if (a.ball) {
      disc(g, 0, 0, 5, "#17130f");
      disc(g, -6, 0, 3, "rgba(255,150,50,.75)");
    } else {
      box(g, -8, -1, 13, 2, "#4a2c17");
      poly(g, "#e8eef0", 4, -3, 10, 0, 4, 3);
    }
    g.restore();
  }
  for (const q of sim.particles) if (canSee(sim, p, q.x, q.y)) box(g, sx(q.x), sy(q.y), q.size, q.size, q.color);

  // fog of war
  for (let ty = y0; ty <= y1; ty++)
    for (let tx = x0; tx <= x1; tx++) {
      const i = ty * COLS + tx;
      if (sim.visible[p][i]) continue;
      const [x, y] = corner(tx, ty, 1, 1);
      box(g, x, y, 32.5, 32.5, sim.explored[p][i] ? "rgba(8,12,10,.38)" : "rgba(8,12,10,.82)");
    }

  g.font = "bold 11px system-ui,sans-serif";
  g.textAlign = "center";
  for (const f of sim.floaters) {
    if (!canSee(sim, p, f.x, f.y)) continue;
    g.fillStyle = f.color;
    g.fillText(f.text, sx(f.x), sy(f.y) - 16);
  }

  const mark = (tx: number, ty: number, w: number, h: number, fill: string, edge: string) => {
    const [x, y] = corner(tx, ty, w, h);
    box(g, x, y, w * 32, h * 32, fill);
    g.strokeStyle = edge;
    g.lineWidth = 2;
    g.strokeRect(x, y, w * 32, h * 32);
  };
  const anchor = sim.place[p] && sim.anchor[p];
  if (anchor) mark(anchor.x, anchor.y, 1, 1, "rgba(255,240,130,.28)", "#fff27a");
  const bad = sim.bad[p];
  if (bad) mark(bad.tx, bad.ty, bad.w, bad.h, "rgba(240,60,40,.4)", "#ff6a55");
}

const MINI = { water: "#2c6f9e", dirt: "#a8864f", grass: "#5c8338" } as const;
const MINI_NODE = { wood: "#1d4a22", food: "#d0365a", gold: "#ffd23f", stone: "#d5d9d4" } as const;
const MINI_FISH = "#f4c9a8";

/** One pixel per tile, turned the same way as the player's own view. */
export function renderMini(g: G, sim: Sim, p: PlayerId) {
  g.setTransform(1, 0, 0, 1, 0, 0);
  if (isNorth(p)) g.setTransform(-1, 0, 0, -1, COLS, ROWS);
  for (let y = 0; y < ROWS; y++)
    for (let x = 0; x < COLS; x++) box(g, x, y, 1, 1, sim.explored[p][y * COLS + x] ? MINI[terrainAt(x, y)] : "#161b13");
  for (const n of sim.nodes) if (!n.farm && n.amount > 0 && sim.explored[p][n.y * COLS + n.x]) box(g, n.x, n.y, 1, 1, n.sea ? MINI_FISH : MINI_NODE[n.kind]);
  for (const b of sim.buildings) {
    if (!(b.seen & (1 << p))) continue;
    const s = BUILDINGS[b.kind];
    box(g, b.tx, b.ty, s.w, s.h, s.wall ? TEAM[b.owner].light : TEAM[b.owner].main);
  }
  for (const u of sim.units) if (u.owner === p || canSee(sim, p, u.x, u.y)) box(g, u.x - 1, u.y - 1, 2, 2, u.owner === p ? "#fff" : TEAM[u.owner].main);
  const v = sim.view[p];
  g.strokeStyle = "#fff27a";
  g.lineWidth = 1;
  g.strokeRect(sim.cam[p].x - v.w / 64, sim.cam[p].y - v.h / 64, v.w / 32, v.h / 32);
}
