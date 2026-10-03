import type { Config } from "tailwindcss";

const token = (name: string) => `rgb(var(--${name}) / <alpha-value>)`;

const config: Config = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: token("ink"),
        panel: token("panel"),
        raised: token("raised"),
        line: token("line"),
        text: token("text"),
        muted: token("muted"),
        accent: token("accent"),
        commit: token("commit"),
        withhold: token("withhold"),
        reject: token("reject"),
        review: token("review"),
        ai: token("ai"),
        hazard: token("hazard"),
      },
      fontFamily: {
        sans: ["ui-sans-serif", "system-ui", "-apple-system", "Segoe UI", "Roboto", "Helvetica Neue", "Arial", "sans-serif"],
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "Consolas", "Liberation Mono", "monospace"],
      },
    },
  },
  plugins: [],
};
export default config;
