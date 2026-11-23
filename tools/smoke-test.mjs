// Opens the built game in headless Chrome (software WebGL), waits for the
// world to appear, walks forward for a few seconds and records what happened.
// Usage: node tools/smoke-test.mjs http://localhost:8080/ out-dir
import puppeteer from "puppeteer-core";
import fs from "node:fs";

const url = process.argv[2] || "http://localhost:8080/";
const out = process.argv[3] || "smoke";
fs.mkdirSync(out, { recursive: true });
const log = [];
const note = (s) => { const line = `[${(performance.now() / 1000).toFixed(1)}s] ${s}`; log.push(line); console.log(line); };

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || "/usr/bin/google-chrome",
  headless: true,
  args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--no-sandbox", "--window-size=1280,720"],
  defaultViewport: { width: 1280, height: 720 },
  // software rendering on a slow runner can keep the page busy for minutes;
  // let the longer waits below run their full time
  protocolTimeout: 900000,
});
let ok = true;
try {
  // first, as a phone (iPhone-sized screen, touch)
  {
    const phone = await browser.newPage();
    phone.on("console", (m) => note(`phone console.${m.type()}: ${m.text()}`));
    phone.on("pageerror", (e) => note(`phone pageerror: ${e.message}`));
    await phone.emulate({
      viewport: { width: 390, height: 844, deviceScaleFactor: 3, isMobile: true, hasTouch: true },
      userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
    });
    phone.setDefaultNavigationTimeout(180000);
    await phone.goto(url, { waitUntil: "load" });
    try {
      await phone.waitForFunction(() => !document.getElementById("loading"), { timeout: 240000, polling: 500 });
      note("phone: world is drawing");
    } catch {
      ok = false;
      note("phone: TIMEOUT waiting for the world");
    }
    await new Promise((r) => setTimeout(r, 25000));
    const size = await phone.evaluate(() => { const c = document.getElementById("bevy"); return `${c.width}x${c.height}`; });
    note(`phone canvas: ${size}`);
    await phone.screenshot({ path: `${out}/0-phone.png` });
    // the map screen on a phone
    try {
      await phone.waitForFunction(() => !!window.wsMap, { timeout: 300000, polling: 500 });
      await phone.click("#map-btn");
      await new Promise((r) => setTimeout(r, 1500));
      await phone.screenshot({ path: `${out}/0b-phone-map.png` });
    } catch (e) {
      ok = false;
      note(`phone map: ${e.message}`);
    }
    await phone.close();
  }

  const page = await browser.newPage();
  page.on("console", (m) => note(`console.${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => note(`pageerror: ${e.message}`));
  page.on("requestfailed", (r) => note(`requestfailed: ${r.url()} ${r.failure()?.errorText}`));
  page.setDefaultNavigationTimeout(180000);
  await page.goto(url, { waitUntil: "load" });
  note("page loaded");
  try {
    await page.waitForFunction(() => !document.getElementById("loading"), { timeout: 240000, polling: 500 });
    note("loading cover removed (frames are drawing)");
  } catch {
    ok = false;
    note("TIMEOUT: loading cover never went away");
  }
  await new Promise((r) => setTimeout(r, 6000));
  await page.screenshot({ path: `${out}/1-start.png` });
  note("screenshot 1-start.png");
  await page.focus("#bevy");
  await page.keyboard.down("KeyW");
  await new Promise((r) => setTimeout(r, 8000));
  await page.keyboard.up("KeyW");
  await new Promise((r) => setTimeout(r, 6000));
  await page.screenshot({ path: `${out}/2-walked.png` });
  note("screenshot 2-walked.png");
  const save = await page.evaluate(() => localStorage.getItem("worldshard-save-v1"));
  note(`save in browser: ${save}`);
  if (!save) note("(no save yet: software rendering is very slow, so little game time has passed)");

  // Load a few saved spots to check night lighting, lava glow and the forests.
  const scenes = [
    ["3-everest-from-650km", { x: -621000, z: 324000, yaw: 0.785, pitch: 0.1, hours: 10 }],
    ["3b-everest-from-300km-flying", { fly: true, y: 40000, x: -338000, z: 41000, yaw: 0.785, pitch: 0.12, hours: 15.5 }],
    ["4-st-helens-from-the-north", { x: 279496, z: -64760, yaw: 3.1416, pitch: 0.12, hours: 15 }],
    ["5-south-forest", { x: 0, z: 991680, yaw: 0, pitch: 0.0, hours: 15 }],
    ["6-whole-island", { fly: true, y: 1400000, x: 0, z: 2600000, yaw: 0, pitch: -0.5, hours: 12 }],
    ["7-northeast-desert", { x: 976000, z: -563500, yaw: 1.047, pitch: 0.02, hours: 11 }],
    ["8-north-highlands", { x: 0, z: -1127000, yaw: 3.27, pitch: 0.04, hours: 9 }],
    ["9-southwest-plains", { x: -1100000, z: 300000, yaw: 1.3, pitch: 0.03, hours: 17 }],
    ["10-everest-evening-shadow", { x: 393776, z: -171304, yaw: 4.712, pitch: 0.1, hours: 17.5 }],
    ["14-cloudwall-at-dawn", { fly: true, y: 2000, x: 1500000, z: 0, yaw: 1.5708, pitch: 0.06, hours: 6.6 }],
    ["15-clouds-overhead", { x: -905738, z: 522928, yaw: 1.05, pitch: 0.55, hours: 13 }],
    ["16-night-sky", { x: -905738, z: 522928, yaw: 2.5, pitch: 0.5, hours: 23 }],
    ["17-range-from-the-west", { x: -900000, z: -150000, yaw: 1.5708, pitch: 0.12, hours: 10 }],
    // the Great Canyon, its bridge city and natural sea wall; rivers, shores and lava
    ["18-bridge-city-street", { x: 1099230, z: -11387, y: 1180, yaw: 2.958, pitch: 0.02, hours: 10 }],
    ["19-bridge-from-the-rim", { fly: true, y: 1700, x: 1111471, z: -22148, yaw: -2.35, pitch: -0.06, hours: 16 }],
    ["20-canyon-floor-looking-up", { fly: true, y: -44200, x: 1040148, z: -1216, yaw: 1.419, pitch: 0.5, hours: 12 }],
    ["21-river-valley", { fly: true, y: 140, x: -1058754, z: 33900, yaw: -1.79, pitch: -0.12, hours: 11 }],
    ["22-northeast-cliffs-and-sea-stacks", { fly: true, y: 250, x: 901755, z: -1339053, yaw: -2.384, pitch: 0.0, hours: 15 }],
    ["23-southwest-beach-surf", { x: -1218486, z: 1022431, yaw: -2.269, pitch: -0.08, hours: 16 }],
    ["24-lava-from-st-helens", { fly: true, y: 4600, x: 431571, z: -109229, yaw: -2.585, pitch: -0.02, hours: 19 }],
    ["25-sea-wall-from-the-sea", { fly: true, y: 6000, x: 1735574, z: 9051, yaw: -1.758, pitch: -0.1, hours: 9 }],
  ];
  for (const [name, p] of scenes) {
    const data = JSON.stringify({ game: "WorldShard", v: 2, day: 2, visited: [], playSec: 0, savedAt: 0, ...p });
    // a scene change is not a crash: clear the start-up flag so the page
    // doesn't drop to lite quality
    await page.evaluate((d) => { localStorage.setItem("worldshard-save-v1", d); sessionStorage.removeItem("worldshard-booting"); }, data);
    await page.reload({ waitUntil: "load" });
    try {
      await page.waitForFunction(() => !document.getElementById("loading"), { timeout: 120000, polling: 500 });
    } catch {
      ok = false;
      note(`TIMEOUT loading ${name}`);
    }
    await new Promise((r) => setTimeout(r, 8000));
    await page.screenshot({ path: `${out}/${name}.png` });
    note(`screenshot ${name}.png`);
  }

  // the map, fast travel to a place, and the settings sliders
  try {
    await page.waitForFunction(() => !!window.wsMap && !!window.wsPlaces, { timeout: 300000, polling: 500 });
    await page.click("#map-btn");
    await new Promise((r) => setTimeout(r, 1500));
    await page.screenshot({ path: `${out}/11-map.png` });
    note("screenshot 11-map.png");
    const target = await page.evaluate(() => {
      const p = JSON.parse(window.wsPlaces).places.find((q) => q.name === "Northeast");
      [...document.querySelectorAll("#map-places .place")].find((b) => b.textContent.startsWith("Northeast")).click();
      return p;
    });
    await page.waitForFunction((t) => {
      const me = (window.wsPlayer || "").split(",").map(Number);
      return Math.hypot(me[0] - t.x, me[1] - t.z) < 2000;
    }, { timeout: 180000, polling: 250 }, target);
    note(`travelled to the Northeast (${target.x}, ${target.z})`);
    await new Promise((r) => setTimeout(r, 12000));
    await page.screenshot({ path: `${out}/12-after-travel-northeast.png` });
    note("screenshot 12-after-travel-northeast.png");
    await page.click("#settings-btn");
    await new Promise((r) => setTimeout(r, 800));
    await page.screenshot({ path: `${out}/13-settings.png` });
    const before = await page.evaluate(() => JSON.parse(window.wsSettings).view);
    await page.evaluate(() => {
      const el = document.getElementById("s-view");
      el.value = 6000;
      el.dispatchEvent(new Event("input"));
    });
    // (software rendering can take several seconds a frame here)
    await page.waitForFunction(() => JSON.parse(window.wsSettings).view === 6000, { timeout: 90000, polling: 250 });
    note(`settings: land view distance ${before} km -> 6000 km applied`);
    // put it back, and close the panels
    await page.evaluate((v) => {
      const el = document.getElementById("s-view");
      el.value = v;
      el.dispatchEvent(new Event("input"));
      document.getElementById("settings-panel").hidden = true;
      document.getElementById("map-panel").hidden = true;
    }, before);
  } catch (e) {
    ok = false;
    note(`map/travel/settings: ${e.message}`);
  }

  // more views without reloading the page: jump there with the travel
  // request (which can also set the view), as fast travel does. These also
  // check that each region's models load as you arrive and are let go
  // after you leave.
  const jumps = [
    ["26-st-helens-ash-fields", { x: 345496, z: 47240, fly: 1, y: 17100, yaw: -2.498, pitch: 0.12, hours: 15 }],
    ["27-northeast-badlands", { x: 927386, z: -557230, yaw: 2.4, pitch: 0.0, hours: 17 }],
    ["28-oasis", { x: 1182661, z: -251187, yaw: -1.571, pitch: -0.08, hours: 11 }, ["Northeast"], ["Southwest", "Northwest"]],
    ["29-south-stream", { x: 21247, z: 951890, fly: 1, y: 808, yaw: -3.022, pitch: -0.15, hours: 13 }],
    ["30-south-clearing", { x: -126106, z: 897290, yaw: 0.0, pitch: 0.0, hours: 14 }],
    ["31-southwest-lake", { x: -1077707, z: 369851, yaw: 1.571, pitch: -0.08, hours: 16 }, ["Southwest"], ["Northeast"]],
    ["32-southeast-valley-fields", { x: 643759, z: 493973, fly: 1, y: 200, yaw: 1.941, pitch: -0.15, hours: 10 }],
    ["33-terrace-lakes-falls", { x: -534510, z: -731526, fly: 1, y: 177, yaw: 0.896, pitch: 0.05, hours: 12 }],
    ["34-terrace-lakes-from-the-top", { x: -534261, z: -731815, yaw: -2.246, pitch: -0.3, hours: 12 }],
    ["35-ruined-outpost", { x: 1101138, z: 3721, yaw: 3.13, pitch: 0.02, hours: 17 }, ["Bridge City"]],
    ["36-the-needles", { x: 279997, z: 1428115, yaw: -1.571, pitch: 0.12, hours: 18 }],
    ["37-north-pass", { x: 395148, z: -698385, fly: 1, y: 4630, yaw: 0.515, pitch: -0.12, hours: 11 }],
    ["38-headland-cliffs", { x: -575013, z: 1595389, fly: 1, y: 90, yaw: 0.75, pitch: 0.0, hours: 16 }],
    ["39-tableland-escarpment", { x: -572326, z: 806442, yaw: 1.571, pitch: 0.06, hours: 15 }],
  ];
  for (const [name, v, mustHave, mustNot] of jumps) {
    try {
      const req = `${Date.now()};${v.x};${v.z};${v.yaw};${v.pitch};${v.hours};${v.fly ? 1 : 0};${v.y ?? 0}`;
      await page.evaluate((r) => localStorage.setItem("worldshard-travel", r), req);
      await page.waitForFunction((x, z) => {
        const p = (window.wsPlayer || "").split(",").map(Number);
        return Math.abs(p[0] - x) < 60 && Math.abs(p[1] - z) < 60;
      }, { timeout: 180000, polling: 500 }, v.x, v.z);
      await new Promise((r) => setTimeout(r, 14000));
      await page.screenshot({ path: `${out}/${name}.png` });
      const loaded = await page.evaluate(() => window.wsLoaded || "");
      note(`screenshot ${name}.png (models loaded: ${loaded})`);
      const have = loaded.split(",");
      for (const want of mustHave || []) {
        if (!have.includes(want)) { ok = false; note(`PROBLEM: ${want} models not loaded at ${name}`); }
      }
      for (const gone of mustNot || []) {
        if (have.includes(gone)) { ok = false; note(`PROBLEM: ${gone} models still loaded at ${name}`); }
      }
    } catch (e) {
      ok = false;
      note(`${name}: ${e.message}`);
    }
  }

} catch (e) {
  ok = false;
  note(`ERROR: ${e.stack || e}`);
} finally {
  fs.writeFileSync(`${out}/smoke.log`, log.join("\n") + `\nresult: ${ok ? "ok" : "PROBLEM"}\n`);
  await browser.close();
}
