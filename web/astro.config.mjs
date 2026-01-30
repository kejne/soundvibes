import { defineConfig } from "astro/config";
import tailwind from "@astrojs/tailwind";

const base = process.env.SV_BASE_PATH ?? "/";

export default defineConfig({
  output: "static",
  base,
  integrations: [tailwind()],
});
