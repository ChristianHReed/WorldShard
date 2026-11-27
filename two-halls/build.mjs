// Bundles the game into one self-contained page: index.html
import { build } from "esbuild";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (path) => fileURLToPath(new URL(path, import.meta.url));
const out = await build({
  entryPoints: [here("src/ui/main.ts")],
  bundle: true,
  format: "iife",
  minify: true,
  target: "es2020",
  write: false,
});
const js = out.outputFiles[0].text.replaceAll("</script", "<\\/script");
const page = readFileSync(here("src/ui/page.html"), "utf8");
writeFileSync(here("index.html"), page.replace("/*GAME*/", () => js));
console.log("wrote index.html");
