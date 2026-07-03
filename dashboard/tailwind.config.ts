import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./app/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        docker: {
          50: "#eaf6ff",
          100: "#d5edff",
          200: "#a9dafb",
          300: "#7ac5f7",
          400: "#4bb0f2",
          500: "#2496ed",
          600: "#1d75c1",
          700: "#155a94",
          800: "#0c2d44",
          900: "#07131c"
        },
        capsulet: {
          bg: "#07131c",
          shell: "#0b1822",
          panel: "#0e1e2a",
          canvas: "#08151f",
          line: "#203241",
          subtle: "#1b2b38",
          muted: "#879daf",
          text: "#dcebf7"
        }
      },
      boxShadow: {
        flat: "0 10px 24px rgba(0, 0, 0, 0.16)"
      }
    }
  },
  plugins: []
};

export default config;
