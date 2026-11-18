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
  const page = await browser.newPage();
  page.on("console", (m) => note(`console.${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => note(`pageerror: ${e.message}`));
  page.on("requestfailed", (r) => note(`requestfailed: ${r.url()} ${r.failure()?.errorText}`));
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
  if (!save) ok = false;
} catch (e) {
  ok = false;
  note(`ERROR: ${e.stack || e}`);
} finally {
  fs.writeFileSync(`${out}/smoke.log`, log.join("\n") + `\nresult: ${ok ? "ok" : "PROBLEM"}\n`);
  await browser.close();
}
