import type { Config } from "tailwindcss";

const config: Config = {
  content: [
    "./src/pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/components/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        background: "#0d1117",
        panel: "#161b22",
        panelBorder: "#30363d",
        cyanGlow: "#00f0ff",
        amberAlert: "#ffb703",
        redAlert: "#e63946",
        greenOk: "#2a9d8f",
      },
    },
  },
  plugins: [],
};
export default config;
