import { playSfx, resumeAudio, setMuted, unlockAudio } from "../game/audio";
import { AGES, BUILDINGS, BUILD_MENU, COLS, ROWS, SEATS, SIZE, TECHS, isNorth, UNITS, canAfford, type Cost, type TechKind, type UnitKind, type BuildingKind } from "../game/content";
import {
  IDS, begin, commandTap, techBlock, unitStats, createSim, demolish, research, focusHall, hasComplete, panCam, clampCam, selectGroup,
  setPlace, step, takeSnap, toggleHunt, togglePause, trainUnit,
  type Mode, type PlayerId, type SideSnap,
} from "../game/sim";
import { render, renderMini } from "./draw";
import { chooseSize, loadPlayers, mapEditor, mapSize, savedMap } from "./editor";
import { sizePicker } from "./sizes";

const sim = createSim();
(window as unknown as { __game: unknown }).__game = sim;
const $ = <T extends HTMLElement = HTMLElement>(q: string) => document.querySelector(q) as T;
const stage = $("#stage");
const overlay = $("#overlay");

/** Three and four players need room for four sets of controls. */
const bigScreen = () => Math.min(window.innerWidth, window.innerHeight) >= 600 && Math.max(window.innerWidth, window.innerHeight) >= 800;

// every seat gets a copy of the same panel
const template = $<HTMLTemplateElement>("#seat");
for (const p of IDS) {
  const panel = (template.content.cloneNode(true) as DocumentFragment).firstElementChild as HTMLElement;
  panel.id = `p${p}`;
  panel.setAttribute("aria-label", `${SEATS[p].name} player`);
  panel.style.setProperty("--team", SEATS[p].main);
  panel.classList.toggle("north", isNorth(p));
  stage.insertBefore(panel, overlay);
}

const panels = IDS.map((p) => {
  const root = $(`#p${p}`);
  const q = <T extends HTMLElement = HTMLElement>(s: string) => root.querySelector(s) as T;
  const canvas = q<HTMLCanvasElement>("canvas");
  return {
    p, root, canvas, g: canvas.getContext("2d")!,
    mini: q<HTMLCanvasElement>(".mini"), zoomBtn: q<HTMLButtonElement>(".zoom"), far: false, cat: -1,
    /** showing the upgrades of a building that also trains */
    ups: 0,
    gone: q(".gone"),
    res: q(".res"), info: q(".info"), actions: q(".actions"), groups: q(".groups"), toast: q(".toast"),
    w: 0, h: 0, zoom: 1, actionKey: "", resKey: "", groupKey: "",
  };
});

const handlers = new WeakMap<Element, () => void>();
let turned = false; // whole stage rotated 90° so the players sit at the left and right ends

const seated = (p: PlayerId) => (sim.mode === "solo" ? p === 0 : p < sim.count);

function layout() {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const quad = sim.mode === "duel" && sim.count > 2;
  turned = sim.mode === "duel" && !quad && vw > vh && matchMedia("(pointer: coarse)").matches;
  document.body.classList.toggle("solo", sim.mode === "solo");
  document.body.classList.toggle("quad", quad);
  document.body.classList.toggle("three", quad && sim.count === 3);
  for (const p of IDS) $(`#p${p}`).hidden = !seated(p);
  document.body.classList.toggle("turned", turned);
  stage.style.width = `${turned ? vh : vw}px`;
  stage.style.height = `${turned ? vw : vh}px`;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  for (const q of panels) {
    const box = q.canvas.parentElement!;
    q.w = box.clientWidth;
    q.h = box.clientHeight;
    if (!q.w || !q.h) continue;
    q.canvas.width = Math.round(q.w * dpr);
    q.canvas.height = Math.round(q.h * dpr);
    q.zoom = Math.max(0.6, Math.min(2, q.w / (12 * 32), q.h / (8 * 32))) * (q.far ? 0.55 : 1);
    sim.view[q.p] = { w: q.w / q.zoom, h: q.h / q.zoom };
    clampCam(sim, q.p);
    // the minimap takes the shape of the map
    if (q.mini.width !== COLS || q.mini.height !== ROWS) {
      q.mini.width = COLS;
      q.mini.height = ROWS;
    }
    const fit = Math.min(68 / COLS, 84 / ROWS);
    q.mini.style.width = `${Math.round(COLS * fit) + 4}px`;
    q.mini.style.height = `${Math.round(ROWS * fit) + 4}px`;
  }
}

/** Screen point -> the canvas's own coordinates (from its centre), undoing our rotations. */
function local(q: (typeof panels)[number], e: PointerEvent) {
  const r = q.canvas.getBoundingClientRect();
  const dx = e.clientX - (r.left + r.width / 2);
  const dy = e.clientY - (r.top + r.height / 2);
  const a = ((turned ? 90 : 0) + (isNorth(q.p) ? 180 : 0)) * (Math.PI / 180);
  return { x: dx * Math.cos(a) + dy * Math.sin(a), y: -dx * Math.sin(a) + dy * Math.cos(a) };
}

for (const q of panels) {
  const touches = new Map<number, { sx: number; sy: number; x: number; y: number; moved: boolean }>();
  const dir = isNorth(q.p) ? -1 : 1;
  q.canvas.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    try { q.canvas.setPointerCapture(e.pointerId); } catch { /* pointer already gone */ }
    const at = local(q, e);
    touches.set(e.pointerId, { sx: at.x, sy: at.y, x: at.x, y: at.y, moved: false });
  });
  q.canvas.addEventListener("pointermove", (e) => {
    const t = touches.get(e.pointerId);
    if (!t || sim.phase !== "play") return;
    const at = local(q, e);
    if (!t.moved && Math.hypot(at.x - t.sx, at.y - t.sy) > 10) t.moved = true;
    if (t.moved) {
      const k = (dir * 1) / (q.zoom * 32);
      panCam(sim, q.p, -(at.x - t.x) * k, -(at.y - t.y) * k);
      sim.uiDirty = true;
    }
    t.x = at.x;
    t.y = at.y;
  });
  q.canvas.addEventListener("pointerup", (e) => {
    const t = touches.get(e.pointerId);
    touches.delete(e.pointerId);
    if (!t || t.moved) return;
    const wx = sim.cam[q.p].x + (dir * t.x) / (q.zoom * 32);
    const wy = sim.cam[q.p].y + (dir * t.y) / (q.zoom * 32);
    commandTap(sim, q.p, wx, wy);
  });
  q.canvas.addEventListener("pointercancel", (e) => touches.delete(e.pointerId));
}

// Minimap: touch or drag it to jump around. Zoom button: near / far.
for (const q of panels) {
  const jump = (e: PointerEvent) => {
    const r = q.mini.getBoundingClientRect();
    const dx = e.clientX - (r.left + r.width / 2);
    const dy = e.clientY - (r.top + r.height / 2);
    const a = ((turned ? 90 : 0) + (isNorth(q.p) ? 180 : 0)) * (Math.PI / 180);
    const lx = (dx * Math.cos(a) + dy * Math.sin(a)) / q.mini.offsetWidth;
    const ly = (-dx * Math.sin(a) + dy * Math.cos(a)) / q.mini.offsetHeight;
    const dir = isNorth(q.p) ? -1 : 1;
    sim.cam[q.p] = { x: (0.5 + dir * lx) * COLS, y: (0.5 + dir * ly) * ROWS };
    clampCam(sim, q.p);
    sim.uiDirty = true;
  };
  let finger: number | null = null;
  q.mini.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    finger = e.pointerId;
    try { q.mini.setPointerCapture(e.pointerId); } catch { /* pointer already gone */ }
    if (sim.phase === "play") jump(e);
  });
  q.mini.addEventListener("pointermove", (e) => { if (e.pointerId === finger && sim.phase === "play") jump(e); });
  for (const end of ["pointerup", "pointercancel"] as const) q.mini.addEventListener(end, () => (finger = null));
  handlers.set(q.zoomBtn, () => {
    q.far = !q.far;
    q.zoomBtn.textContent = q.far ? "+" : "−";
    layout();
  });
}

// Buttons fire on pointerdown so both players can press at the same moment.
document.addEventListener("pointerdown", (e) => {
  unlockAudio();
  setMuted(!sim.sound);
  const b = (e.target as Element).closest("#stage button");
  if (b?.closest("#overlay")) return;
  const fn = b && handlers.get(b);
  if (!fn) return;
  e.preventDefault();
  fn();
  sim.uiDirty = true;
});
document.addEventListener("pointerup", () => resumeAudio());
document.addEventListener("contextmenu", (e) => e.preventDefault());

function button(label: string, fn: () => void, sub = "", cls = "") {
  const b = document.createElement("button");
  b.type = "button";
  b.className = cls;
  b.innerHTML = `<b>${label}</b>${sub ? `<small>${sub}</small>` : ""}`;
  handlers.set(b, fn);
  // keyboard activation (pointer presses are handled above)
  b.addEventListener("click", (e) => {
    if (b.closest("#overlay")) {
      fn();
      sim.uiDirty = true;
    } else if (e.detail === 0) fn();
  });
  return b;
}

const cost = (c: Cost) => Object.entries(c).map(([k, v]) => `<i class="${k}">${v}${k[0]!.toUpperCase()}</i>`).join(" ");

function hud(p: PlayerId, s: SideSnap) {
  const q = panels[p]!;
  const idle = sim.units.filter((u) => u.owner === p && (u.kind === "villager" || u.kind === "fisher") && u.order.t === "idle").length;
  const resKey = [s.food, s.wood, s.gold, s.stone, s.pop, s.cap, s.age].join();
  if (resKey !== q.resKey) {
    q.resKey = resKey;
    q.res.innerHTML =
      (["food", "wood", "gold", "stone"] as const).map((k) => `<span><i class="${k}">${k}</i> ${s[k]}</span>`).join("") +
      `<span><i>pop</i> ${s.pop}/${s.cap}</span><span class="age" title="${AGES[s.age]} age">${"I".repeat(s.age)}</span>`;
  }
  const b = sim.buildings.find((o) => o.id === sim.sel[p].building);
  const folkPicked = !s.place && (s.sel === "folk" || s.sel === "mix");
  if (!folkPicked && !s.place) q.cat = -1;
  if (q.ups !== (b?.id ?? 0)) q.ups = 0;
  // each building offers the first upgrade of every chain that is not finished yet
  const upgrades = b && b.owner === p
    ? (Object.keys(TECHS) as TechKind[]).filter((k) => {
        const t = TECHS[k];
        if (t.at !== b.kind) return false;
        if (t.after && !sim.tech[p].includes(t.after)) return false;
        return !(sim.tech[p].includes(k) && (Object.values(TECHS).some((n) => n.after === k) || k === "age3"));
      })
    : [];
  const upsView = !!b && b.progress >= 1 && upgrades.length > 0 && b.kind !== "town" && (!s.train.length || q.ups === b.id);
  let info = "Tap one of your people. Drag the map to look around.";
  if (s.place) info = s.placeReason;
  else if (b) {
    info = `${s.selName} · health ${s.hp}`;
    if (b.progress < 1) info += ` · ${Math.floor(b.progress * 100)}% built`;
    else if (b.research) info += ` · ${TECHS[b.research.tech].name} in ${Math.ceil(b.research.left)}s`;
    else if (b.queue.length) info += ` · training ${b.queue.length}, next ${Math.ceil(b.queue[0]!.left)}s`;
    else if (upsView) info = upgrades.filter((k) => !sim.tech[p].includes(k)).map((k) => `${TECHS[k].name}: ${TECHS[k].effect}`).join(" · ") || `${s.selName}: every upgrade done`;
    else if (b.kind === "town") {
      const next = upgrades.find((k) => !sim.tech[p].includes(k));
      if (next) {
        const missing = TECHS[next].needs && !TECHS[next].needs!.some((k) => hasComplete(sim, p, k));
        info = `${AGES[s.age]} age. ${TECHS[next].name} ${missing ? `needs a ${TECHS[next].needs!.map((k) => BUILDINGS[k].name.toLowerCase()).join(" or ")}` : TECHS[next].effect}`;
      } else info = `Castle age · health ${s.hp}`;
    } else if (b.kind === "gate") info += " · only your people pass";
  } else if (s.selCount) {
    const todo = { folk: "tap a resource, a damaged building or the ground", boats: "tap fish or water", host: "tap an enemy or where to go", mix: "tap where to go", none: "", hall: "" };
    info = `${s.selName} · ${todo[s.sel]}`;
  }
  // upgrade effects and age advice may need a second line
  const tall = !!b && !s.place && (upsView || b.kind === "town") && !b.research && !b.queue.length && b.progress >= 1;
  q.info.classList.toggle("tall", tall);
  if (q.info.textContent !== info) q.info.textContent = info;
  q.toast.textContent = s.note;
  q.toast.hidden = !s.note;

  const can = (c: Cost) => (canAfford(sim.res[p], c) ? "" : "poor");
  const open = (k: BuildingKind) => { const n = BUILDINGS[k].need; return BUILDINGS[k].age <= s.age && (!n || hasComplete(sim, p, n)); };
  const lockedWhy = (k: BuildingKind) => (BUILDINGS[k].age > s.age ? `${AGES[BUILDINGS[k].age]} age` : `needs ${BUILDINGS[BUILDINGS[k].need!].name}`);
  const say = (text: string) => (sim.note[p] = { text, t: 2.4 });
  const shown: Cost[] = [
    ...(q.cat >= 0 ? BUILD_MENU[q.cat]!.kinds.map((k) => BUILDINGS[k].cost) : []),
    ...s.train.map((k) => UNITS[k].cost),
    ...upgrades.map((k) => TECHS[k].cost),
    ...(s.place ? [BUILDINGS[s.place].cost] : []),
  ];
  const key = [
    s.place, s.anchored, s.sel, s.building, sim.sel[p].building, s.train.join(), s.hunt, q.cat, q.ups, s.age,
    upgrades.map((k) => `${k}${techBlock(sim, p, k)}`).join(), shown.map(can).join(),
    q.cat >= 0 ? BUILD_MENU[q.cat]!.kinds.map(open).join() : "", b && b.progress >= 1, sim.tech[p].join(),
  ].join("|");
  if (key !== q.actionKey) {
    q.actionKey = key;
    const list: HTMLElement[] = [];
    const redo = () => (q.actionKey = "");
    if (s.place) {
      const spec = BUILDINGS[s.place];
      list.push(button(`${spec.name}`, () => {}, `${cost(spec.cost)}${spec.wall && s.place !== "gate" ? " each" : ""}`, `label ${can(spec.cost)}`));
      if (s.anchored) list.push(button("New line", () => { sim.anchor[p] = null; redo(); }, "start elsewhere"));
      list.push(button("Done", () => setPlace(sim, p, null), "", "go"));
    } else if (b && b.owner === p) {
      const upgradeButton = (k: TechKind) => {
        const t = TECHS[k];
        const no = techBlock(sim, p, k);
        const sub = no === "Done" ? "done ✓" : no === "Under way" ? "working…" : t.age > s.age ? `${AGES[t.age]} age` : cost(t.cost);
        const aging = k === "age2" || k === "age3";
        return button(aging ? `▲ ${t.name}` : t.name, () => research(sim, p, k), sub, no ? "poor" : can(t.cost) || (aging ? "go" : ""));
      };
      if (b.progress >= 1) {
        if (upsView) {
          if (s.train.length) list.push(button("◂", () => { q.ups = 0; redo(); }, "", "x"));
          list.push(...upgrades.map(upgradeButton));
        } else {
          for (const k of s.train as UnitKind[]) {
            const spec = UNITS[k];
            const locked = spec.age > s.age;
            const name = b.kind === "dock" ? spec.name.replace(/ (boat|ship)$/i, "") : `Train ${spec.name}`;
            list.push(button(name, () => trainUnit(sim, p, k), locked ? `${AGES[spec.age]} age` : cost(spec.cost), locked ? "poor" : can(spec.cost)));
          }
          if (b.kind === "town") list.push(...upgrades.map(upgradeButton));
          else if (upgrades.length) list.push(button("Upgrades", () => { q.ups = b.id; redo(); }, "▸", upgrades.some((k) => !techBlock(sim, p, k) && canAfford(sim.res[p], TECHS[k].cost)) ? "alert" : ""));
        }
      }
      if (b.kind !== "town" && !(upsView && s.train.length))
        list.push(button(b.progress < 1 ? "Cancel build" : "Remove", () => demolish(sim, p), b.progress < 1 ? "refund" : "", "danger"));
    } else if (folkPicked) {
      if (q.cat < 0) {
        BUILD_MENU.forEach((m, i) => list.push(button(`${m.name} ▸`, () => { q.cat = i; redo(); }, m.hint)));
      } else {
        list.push(button("◂", () => { q.cat = -1; redo(); }, "", "x"));
        for (const k of BUILD_MENU[q.cat]!.kinds) {
          const spec = BUILDINGS[k];
          const ok = open(k);
          list.push(button(spec.name, () => {
            if (ok) setPlace(sim, p, k);
            else say(spec.age > s.age ? `Reach the ${AGES[spec.age]} age at your Hall first` : `Build a ${BUILDINGS[spec.need!].name} first`);
          }, ok ? cost(spec.cost) : lockedWhy(k), ok ? can(spec.cost) : "poor"));
        }
      }
    } else if (s.sel === "host") {
      list.push(button(`Attack-move: ${s.hunt ? "ON" : "OFF"}`, () => toggleHunt(sim, p), "fight enemies met on the way", s.hunt ? "go" : ""));
    }
    q.actions.classList.toggle("many", list.length > 4);
    q.actions.replaceChildren(...list);
  }
  const groupKey = `${idle}|${s.ships > 0}`;
  if (groupKey !== q.groupKey) {
    q.groupKey = groupKey;
    q.groups.replaceChildren(
      button("All Folk", () => { q.cat = -1; q.actionKey = ""; setPlace(sim, p, null); selectGroup(sim, p, "folk"); }),
      button(`Idle ${idle}`, () => { setPlace(sim, p, null); selectGroup(sim, p, "idle"); }, "", idle ? "alert" : ""),
      button("Army", () => { setPlace(sim, p, null); selectGroup(sim, p, "host"); }),
      ...(s.ships ? [button("Fleet", () => { setPlace(sim, p, null); selectGroup(sim, p, "fleet"); })] : []),
      button("Hall", () => {
        const h = sim.buildings.find((o) => o.owner === p && o.kind === "town");
        if (!h) return;
        if (!sim.place[p]) sim.sel[p] = { units: [], building: h.id };
        focusHall(sim, p);
      }),
      button("✕", () => { setPlace(sim, p, null); sim.sel[p] = { units: [], building: null }; }, "", "x"),
    );
  }
}

function start(mode: Mode, players = sim.count) {
  const count = mode === "solo" ? 2 : players;
  loadPlayers(count);
  begin(sim, mode, count);
  for (const q of panels) {
    q.actionKey = q.resKey = q.groupKey = "";
    q.cat = -1;
    q.ups = 0;
    q.far = false;
    q.zoomBtn.textContent = "−";
  }
  screen = "";
  layout();
  for (const p of IDS) if (seated(p)) focusHall(sim, p);
  needFocusDone();
}
function needFocusDone() { sim.needFocus = false; sim.uiDirty = true; }

function toMenu() {
  sim.phase = "menu";
  sim.mode = null;
  layout();
}

const HELP = `
<ol>
<li><b>Move around.</b> Drag the map, or touch the minimap to jump. − / + changes zoom.</li>
<li><b>Gather.</b> Tap a Folk (or <b>All Folk</b>), then trees, berries, gold or stone. Double-tap a unit to pick all of its kind. A Storehouse shortens long walks.</li>
<li><b>Build.</b> With Folk picked, choose a building and <b>tap the map where you want it</b>. Any clear ground works.</li>
<li><b>Walls.</b> Pick Palisade or Stone wall, tap a start, then tap again: the wall runs to that spot. Keep tapping for corners. Put a <b>Gate</b> on open ground or on your wall: only your people can pass.</li>
<li><b>Ages.</b> Tap your Hall to move from the <b>Village</b> to the <b>Town</b> age (needs a Barracks or Dock), then the <b>Castle</b> age (needs a Smithy). Each unlocks new buildings, units and upgrades.</li>
<li><b>Army.</b> Barracks (Levy). Town age: Range (Archer), Stable (fast Rider). Castle age: Workshop (Ram, to break walls and buildings).</li>
<li><b>Ships.</b> Build a <b>Dock</b> on any shore. Fishing boats gather fish and bring it to your Dock. <b>Galleys</b> (Town age) fight ships and shore targets. <b>Bombard ships</b> (Castle age) smash coastal buildings from far off.</li>
<li><b>Upgrades.</b> Tap a Storehouse, Smithy, Barracks, Range, Stable or Dock.</li>
<li><b>Repair</b> by picking Folk and tapping a damaged building.</li>
<li><b>Win</b> by having the last Hall standing. With 3 or 4 players it is everyone for themselves.</li>
</ol>`;

let screen = "";
let editing = false;
function drawOverlay() {
  const want = sim.phase === "play" ? "" : sim.phase;
  const key = `${want}|${sim.sound}|${sim.winner}|${editing}`;
  if (key === screen) return;
  screen = key;
  overlay.hidden = !want;
  overlay.className = want;
  overlay.replaceChildren();
  if (!want) return;
  const card = document.createElement("div");
  card.className = "card";
  const sound = () => button(`Sound: ${sim.sound ? "on" : "off"}`, () => { sim.sound = !sim.sound; setMuted(!sim.sound); drawOverlay(); });
  if (want === "menu" && editing) {
    overlay.append(mapEditor({ button, bigScreen: bigScreen(), close: () => { editing = false; sim.uiDirty = true; } }));
    return;
  }
  if (want === "menu") {
    card.innerHTML = `<h1>Two Halls</h1><p>Gather, build and march to war.<br>Share one screen with up to four players.</p>`;
    const big = bigScreen();
    const more = document.createElement("div");
    more.className = "more";
    for (const n of [3, 4])
      more.append(button(`${n} players`, () => (big ? start("duel", n) : undefined), big ? "two at each end" : "needs a tablet or bigger", big ? "go" : "poor"));
    card.append(button("Two players", () => start("duel", 2), "one at each end", "go"), more, button("Play the computer", () => start("solo")),
      button("Make a map from a picture", () => { editing = true; }, savedMap() ? "now playing on: your picture map" : "now playing on: built-in map"),
      sizePicker({ button, limits: SIZE, current: mapSize, choose: chooseSize }), sound());
    const how = document.createElement("div");
    how.innerHTML = HELP;
    card.append(how);
  } else if (want === "pause") {
    card.innerHTML = `<h2>Paused</h2>`;
    card.append(button("Resume", () => togglePause(sim), "", "go"), button("Restart", () => start(sim.mode!)), button("Main menu", toMenu), sound());
  } else {
    const verdict = (p: PlayerId) => (sim.winner === -1 ? "Draw" : sim.winner === p ? "Victory!" : "Defeat");
    if (sim.mode === "solo") card.innerHTML = `<h2>${verdict(0)}</h2>`;
    else if (sim.count === 2) card.innerHTML = `<h2 class="flip">${verdict(1)}</h2><h2>${verdict(0)}</h2>`;
    else {
      const line = sim.winner === -1 || sim.winner === null ? "Draw" : `${SEATS[sim.winner].name} wins!`;
      card.innerHTML = `<h2 class="flip">${line}</h2><h2>${line}</h2>`;
    }
    card.append(button("Rematch", () => start(sim.mode!), "", "go"), button("Main menu", toMenu));
  }
  overlay.append(card);
}

handlers.set($("#pause"), () => togglePause(sim));
document.addEventListener("visibilitychange", () => { if (document.hidden && sim.phase === "play") togglePause(sim); });
let resizeQueued = false;
window.addEventListener("resize", () => {
  if (resizeQueued) return;
  resizeQueued = true;
  requestAnimationFrame(() => { resizeQueued = false; layout(); });
});

if (typeof ResizeObserver !== "undefined") {
  const watch = new ResizeObserver(() => {
    if (panels.some((q) => seated(q.p) && (q.canvas.parentElement!.clientWidth !== q.w || q.canvas.parentElement!.clientHeight !== q.h))) layout();
  });
  for (const q of panels) watch.observe(q.canvas.parentElement!);
}

let last = performance.now();
let hudAt = 0;
let miniAt = 0;
let was = sim.phase;
function frame(now: number) {
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  for (let left = dt; left > 0; left -= 1 / 30) step(sim, Math.min(left, 1 / 30));
  for (const name of sim.fxQueue.splice(0)) if (sim.sound) playSfx(name);
  if (sim.phase === "end" && was !== "end" && sim.sound) playSfx(sim.mode === "solo" && sim.winner !== 0 ? "lose" : "win");
  was = sim.phase;
  drawOverlay();
  if (sim.phase !== "menu") {
    for (const q of panels) {
      if (!seated(q.p)) continue;
      q.gone.hidden = !sim.out[q.p];
      q.root.classList.toggle("out", sim.out[q.p]);
      render(q.g, sim, q.p, q.w, q.h, q.zoom, q.canvas.width / q.w, (u) => unitStats(sim, u.owner, u.kind).hp);
    }
    if (now - miniAt > 250) {
      miniAt = now;
      for (const q of panels) if (seated(q.p)) renderMini(q.mini.getContext("2d")!, sim, q.p);
    }
    if (sim.uiDirty || now - hudAt > 150) {
      hudAt = now;
      sim.uiDirty = false;
      const snap = takeSnap(sim);
      for (const p of IDS) if (seated(p)) hud(p, snap.players[p]);
    }
  }
  const mm = Math.floor(sim.clock / 60);
  $("#clock").textContent = `${mm}:${String(Math.floor(sim.clock % 60)).padStart(2, "0")}`;
  requestAnimationFrame(frame);
}
layout();
requestAnimationFrame(frame);
