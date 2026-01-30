/** @type {import("tailwindcss").Config} */
export default {
  content: ["./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}"],
  theme: {
    extend: {
      colors: {
        ink: "#0a0f1f",
        "ink-soft": "#111a2e",
        mist: "#dbe7ff",
        cloud: "#f5f7ff",
        mint: "#1ee3b1",
        citrus: "#ffb347",
        coral: "#ff7a7a",
      },
      fontFamily: {
        display: ["\"Space Grotesk\"", "system-ui", "sans-serif"],
        body: ["\"DM Sans\"", "system-ui", "sans-serif"],
      },
      boxShadow: {
        glow: "0 20px 60px rgba(30, 227, 177, 0.25)",
      },
    },
  },
  plugins: [],
};
