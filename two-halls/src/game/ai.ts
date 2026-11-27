import { BUILDINGS, TECHS, UNITS, canAfford, type BuildingKind, type TechKind, type UnitKind } from "./content";
import { ageOf, aiTryBuild, canReachNode, techBlock, hasComplete, orderGather, popCap, popUsed, research, trainUnit, type Node, type PlayerId, type Sim } from "./sim";

type Kind = Node["kind"];

export function aiThink(sim: Sim) {
  if (sim.mode !== "solo" || sim.phase !== "play") return;
  const p: PlayerId = 1;
  const mine = sim.buildings.filter((b) => b.owner === p && b.hp > 0);
  const folks = sim.units.filter((u) => u.owner === p && u.hp > 0 && u.kind === "villager");
  const military = sim.units.filter((u) => u.owner === p && u.hp > 0 && u.kind !== "villager" && !UNITS[u.kind].naval);
  const fishers = sim.units.filter((u) => u.owner === p && u.hp > 0 && u.kind === "fisher");
  const warships = sim.units.filter((u) => u.owner === p && u.hp > 0 && UNITS[u.kind].naval && u.kind !== "fisher");
  const age = ageOf(sim, p);
  const hall = mine.find((b) => b.kind === "town" && b.progress >= 1);
  if (!hall) return;
  const home = { x: hall.tx + 1.5, y: hall.ty + 1.5 };

  const nearestNode = (kind: Kind, x: number, y: number) => {
    let best: Node | null = null;
    let bd = 1e9;
    for (const n of sim.nodes) {
      if (n.kind !== kind || n.sea || (!n.farm && n.amount <= 0)) continue;
      const d = Math.hypot(n.x + 0.5 - x, n.y + 0.5 - y);
      // stay near home, and skip anything walled in by forest or water
      if (d < bd && d < 38 && (!folks[0] || canReachNode(sim, folks[0], n))) {
        bd = d;
        best = n;
      }
    }
    return best;
  };

  const tally: Record<Kind, number> = { food: 0, wood: 0, gold: 0, stone: 0 };
  const job = (u: (typeof folks)[number]) => {
    if (u.order.t !== "gather" && u.order.t !== "drop") return null;
    const id = u.order.node;
    return sim.nodes.find((n) => n.id === id)?.kind ?? null;
  };
  for (const u of folks) {
    const k = job(u);
    if (k) tally[k] += 1;
  }
  const want: Record<Kind, number> = { food: 4, wood: 4, gold: 3, stone: 1 };
  const order: Kind[] = ["wood", "food", "gold", "stone"];
  const shortest = () => order.reduce((a, b) => (tally[a] / want[a] <= tally[b] / want[b] ? a : b));
  const lacking = order.find((k) => tally[k] === 0);
  if (lacking && folks.length >= 4 && !folks.some((u) => u.order.t === "idle")) {
    const spare = folks.find((u) => u.carry < 1 && u.order.t === "gather" && tally[job(u) ?? lacking] > 1);
    if (spare) spare.order = { t: "idle" };
  }
  for (const u of folks) {
    if (u.order.t !== "idle") continue;
    const need = shortest();
    const node = nearestNode(need, home.x, home.y) ?? nearestNode("wood", home.x, home.y);
    if (!node) continue;
    orderGather(sim, [u], node.id);
    tally[need] += 1;
  }

  const count = (k: BuildingKind) => mine.filter((b) => b.kind === k).length;

  // The next age comes first: once it is due, stop spending food until it is paid for.
  const nextAge: TechKind | null = age === 1 && sim.clock > 150 && folks.length >= 8 ? "age2" : age === 2 && sim.clock > 330 && folks.length >= 10 ? "age3" : null;
  const wanted = nextAge && !techBlock(sim, p, nextAge) ? nextAge : null;
  const saving = !!wanted && !canAfford(sim.res[p], TECHS[wanted].cost);

  // the closest fishing water to home decides where a dock goes
  const shoal = sim.nodes
    .filter((n) => n.sea && n.amount > 0)
    .map((n) => ({ n, d: Math.hypot(n.x - home.x, n.y - home.y) }))
    .filter((o) => o.d < 30)
    .sort((a, b) => a.d - b.d)[0]?.n;
  if (!mine.some((b) => b.progress < 1)) {
    const wood = sim.res[p].wood;
    // a storehouse beside any resource that has become a long walk
    let far: Node | null = null;
    if (wood >= 80 && count("store") < 3) {
      for (const k of ["wood", "gold"] as const) {
        const n = nearestNode(k, home.x, home.y);
        if (!n) continue;
        const walk = Math.min(...mine.filter((b) => b.kind === "town" || b.kind === "store").map((b) => Math.hypot(b.tx - n.x, b.ty - n.y)));
        if (walk > 12) far = n;
      }
    }
    if (popUsed(sim, p) >= popCap(sim, p) - 2 && popCap(sim, p) < 45) aiTryBuild(sim, p, "house");
    else if (far) aiTryBuild(sim, p, "store", far);
    else if (count("farm") < 2 && folks.length >= 5) aiTryBuild(sim, p, "farm");
    else if (!count("barracks") && folks.length >= 6) aiTryBuild(sim, p, "barracks");
    else if (!count("dock") && shoal && sim.clock > 120 && wood >= 120 && aiTryBuild(sim, p, "dock", shoal)) {
      // placed
    } else if (age >= 2 && hasComplete(sim, p, "barracks") && !count("range")) aiTryBuild(sim, p, "range");
    else if (age >= 2 && hasComplete(sim, p, "range") && !count("smithy")) aiTryBuild(sim, p, "smithy");
    else if (age >= 2 && sim.clock > 260 && !count("tower")) aiTryBuild(sim, p, "tower");
    else if (age >= 2 && sim.clock > 280 && !count("stable") && wood >= 200) aiTryBuild(sim, p, "stable");
    else if (age >= 3 && !count("workshop") && wood >= 220) aiTryBuild(sim, p, "workshop");
  }

  for (const u of fishers) {
    if (u.order.t !== "idle") continue;
    let best: Node | null = null;
    let bd = 1e9;
    for (const n of sim.nodes) {
      if (!n.sea || n.amount <= 0) continue;
      const d = Math.hypot(n.x - u.x, n.y - u.y);
      if (d < bd && canReachNode(sim, u, n)) {
        bd = d;
        best = n;
      }
    }
    if (best) orderGather(sim, [u], best.id);
  }

  const keep = sim.sel[p];
  const train = (kind: BuildingKind, unit: UnitKind) => {
    const b = mine.find((o) => o.kind === kind && o.progress >= 1 && o.queue.length < 2);
    if (!b) return;
    sim.sel[p] = { units: [], building: b.id };
    trainUnit(sim, p, unit);
  };
  const research1 = (at: BuildingKind, tech: TechKind) => {
    const b = mine.find((o) => o.kind === at && o.progress >= 1 && !o.research);
    if (!b || techBlock(sim, p, tech) || !canAfford(sim.res[p], TECHS[tech].cost)) return;
    sim.sel[p] = { units: [], building: b.id };
    research(sim, p, tech);
  };
  if (wanted) research1("town", wanted);
  if (folks.length + hall.queue.length < 12 && !(saving && folks.length >= 9)) train("town", "villager");
  const have = (k: UnitKind) => military.filter((u) => u.kind === k).length;
  if (fishers.length < 3) train("dock", "fisher");
  if (!saving || military.length < 4) {
    if (military.length < 20) {
      if (have("ram") < 2) train("workshop", "ram");
      if (have("rider") < 4) train("stable", "rider");
      if (have("archer") < 6) train("range", "archer");
      if (have("militia") < 8) train("barracks", "militia");
    }
    if (sim.clock > 300) {
      if (warships.filter((u) => u.kind === "bombard").length < 2) train("dock", "bombard");
      if (warships.filter((u) => u.kind === "galley").length < 3) train("dock", "galley");
    }
    if (sim.clock > 220) {
      for (const t of Object.keys(TECHS) as TechKind[]) if (t !== "age2" && t !== "age3") research1(TECHS[t].at, t);
    }
  }
  sim.sel[p] = keep;

  // Warships hunt the enemy's docks and boats, wherever the water lets them go.
  if (warships.length >= 2) {
    const prey =
      sim.buildings.find((b) => b.owner === 0 && b.kind === "dock" && b.hp > 0 && b.seen & (1 << p)) ??
      null;
    const boat = sim.units.find((u) => u.owner === 0 && u.hp > 0 && UNITS[u.kind].naval);
    const goal = prey ? { x: prey.tx + 1, y: prey.ty + 1 } : boat ? { x: boat.x, y: boat.y } : null;
    if (goal) {
      for (const u of warships) {
        if (u.order.t !== "idle") continue;
        u.order = { t: "move", ...goal, hunt: true };
        u.goalKey = "";
      }
    }
  }

  // Attack in waves: gather a group, then commit it until it is spent.
  if (!sim.aiAttack && ((sim.clock > 330 && military.length >= 8) || (sim.clock > 540 && military.length >= 4))) sim.aiAttack = true;
  if (sim.aiAttack && military.length <= 2) sim.aiAttack = false;
  if (sim.aiAttack) {
    const enemy = sim.buildings.find((b) => b.owner === 0 && b.kind === "town" && b.hp > 0);
    if (!enemy) return;
    for (const u of military) {
      if (u.order.t !== "idle") continue;
      u.order = { t: "move", x: enemy.tx + BUILDINGS.town.w / 2, y: enemy.ty - 0.5, hunt: true };
      if (Math.hypot(enemy.tx - u.x, enemy.ty - u.y) < 6 || u.kind === "ram") u.order = { t: "attack", kind: "building", id: enemy.id, resume: null };
      u.goalKey = "";
    }
  }
}
