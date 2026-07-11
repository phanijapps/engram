/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Dark IDE palette.
        base: {
          950: "#0a0e14",
          900: "#0d1117",
          850: "#11151c",
          800: "#161b22",
          750: "#1c2230",
          700: "#21262d",
          600: "#30363d",
        },
        ink: {
          DEFAULT: "#c9d1d9",
          muted: "#8b949e",
          faint: "#6e7681",
        },
        accent: {
          DEFAULT: "#58a6ff",
          cyan: "#39c5cf",
          purple: "#bc8cff",
          green: "#3fb950",
          amber: "#d29922",
          red: "#f85149",
        },
      },
      fontFamily: {
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
    },
  },
  plugins: [],
};
