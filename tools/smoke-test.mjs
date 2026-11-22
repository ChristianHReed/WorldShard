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
    ["5-south-forest", { x: 0, z: 991760, yaw: 0, pitch: 0.0, hours: 15 }],
    ["6-whole-island", { fly: true, y: 1400000, x: 0, z: 2600000, yaw: 0, pitch: -0.5, hours: 12 }],
    ["7-northeast-desert", { x: 976000, z: -563500, yaw: 1.047, pitch: 0.02, hours: 11 }],
    ["8-north-highlands", { x: 0, z: -1127000, yaw: 3.27, pitch: 0.04, hours: 9 }],
    ["9-southwest-plains", { x: -1100000, z: 300000, yaw: 1.3, pitch: 0.03, hours: 17 }],
  ];
  for (const [name, p] of scenes) {
    const data = JSON.stringify({ game: "WorldShard", v: 2, day: 2, visited: [], playSec: 0, savedAt: 0, ...p });
    await page.evaluate((d) => localStorage.setItem("worldshard-save-v1", d), data);
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
} catch (e) {
  ok = false;
  note(`ERROR: ${e.stack || e}`);
} finally {
  fs.writeFileSync(`${out}/smoke.log`, log.join("\n") + `\nresult: ${ok ? "ok" : "PROBLEM"}\n`);
  await browser.close();
}
