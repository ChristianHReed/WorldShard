// Starts the WebGPU build (the graphics path phones use) in headless Chrome,
// with software WebGPU (SwiftShader through Vulkan), at phone size. It takes
// a screenshot plain and with each test switch, so a black picture can be
// traced to one setting. Runs before the long smoke test, so it reports early.
// Usage: node tools/webgpu-check.mjs http://localhost:8080/ out-dir
import puppeteer from "puppeteer-core";
import fs from "node:fs";

const base = process.argv[2] || "http://localhost:8080/";
const out = process.argv[3] || "webgpu-check";
fs.mkdirSync(out, { recursive: true });
const log = [];
const note = (s) => { const line = `[${(performance.now() / 1000).toFixed(1)}s] ${s}`; log.push(line); console.log(line); };

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || "/usr/bin/google-chrome",
  headless: true,
  args: [
    "--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-vulkan=swiftshader",
    "--use-webgpu-adapter=swiftshader", "--use-angle=swiftshader", "--enable-unsafe-swiftshader",
    "--ignore-gpu-blocklist", "--no-sandbox", "--window-size=1280,720",
  ],
  protocolTimeout: 900000,
});

// [screenshot name, extra address switches]
const runs = [
  ["1-webgpu-phone", ""],
  ["2-webgpu-msaa-off", "&msaa=0"],
  ["3-webgpu-tonemap-off", "&tonemap=0"],
  ["4-webgpu-indirect-off", "&indirect=0"],
];

let ok = true;
try {
  for (const [name, extra] of runs) {
    const page = await browser.newPage();
    page.on("console", (m) => {
      const t = m.text();
      // keep errors, warnings and the game's own lines; skip engine chatter
      if (m.type() === "error" || m.type() === "warn" || /WARN|ERROR|WorldShard|start-up/.test(t)) note(`${name} console.${m.type()}: ${t.slice(0, 400)}`);
    });
    page.on("pageerror", (e) => note(`${name} pageerror: ${e.message}`));
    await page.emulate({
      viewport: { width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true },
      userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
    });
    page.setDefaultNavigationTimeout(180000);
    await page.goto(`${base}?gfx=webgpu${extra}`, { waitUntil: "load" });
    if (name === runs[0][0]) {
      const adapter = await page.evaluate(async () => {
        if (!navigator.gpu) return "no navigator.gpu";
        const a = await navigator.gpu.requestAdapter().catch((e) => null);
        if (!a) return "no adapter";
        const i = a.info || {};
        return `adapter: ${i.vendor || "?"} ${i.architecture || ""} ${i.description || ""}`.trim();
      });
      note(`WebGPU in this Chrome: ${adapter}`);
      if (!adapter.startsWith("adapter")) { ok = false; await page.close(); break; }
    }
    try {
      await page.waitForFunction(() => !document.getElementById("loading"), { timeout: 360000, polling: 1000 });
      note(`${name}: world drawing (game using ${await page.evaluate(() => window.wsGraphics)})`);
    } catch {
      ok = false;
      note(`${name}: TIMEOUT waiting for the world`);
    }
    await new Promise((r) => setTimeout(r, 15000));
    await page.screenshot({ path: `${out}/${name}.png` });
    note(`screenshot ${name}.png`);
    await page.close();
  }
} catch (e) {
  ok = false;
  note(`ERROR: ${e.stack || e}`);
} finally {
  fs.writeFileSync(`${out}/webgpu.log`, log.join("\n") + `\nresult: ${ok ? "ok" : "PROBLEM"}\n`);
  await browser.close();
}
