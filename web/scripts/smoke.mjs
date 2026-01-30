import { readFile } from "node:fs/promises";
import { join } from "node:path";

const distPath = join(process.cwd(), "dist", "index.html");

const mustContain = [
  "Soundvibes",
  "Minimal dependencies",
  "Auto model download",
  "sv daemon start",
  "GitHub Releases",
];

try {
  const html = await readFile(distPath, "utf-8");

  const missing = mustContain.filter((token) => !html.includes(token));
  if (missing.length > 0) {
    console.error("Smoke check failed. Missing tokens:");
    for (const token of missing) {
      console.error(`- ${token}`);
    }
    process.exit(1);
  }

  console.log("Smoke check passed.");
} catch (error) {
  console.error("Smoke check failed.");
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
