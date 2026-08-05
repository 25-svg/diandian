import flowbitePlugin from "flowbite/plugin";

export default {
  content: [
    "./src/**/*.{html,js,svelte,ts}",
    "./node_modules/flowbite-svelte/**/*.{html,js,svelte,ts}",
  ],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Apple system blue (replaces Flowbite orange primary)
        primary: {
          50: "#f0f7ff",
          100: "#e0effe",
          200: "#b9ddfe",
          300: "#7cc2fd",
          400: "#36a4f9",
          500: "#0071e3",
          600: "#0062c7",
          700: "#004e9f",
          800: "#064383",
          900: "#0b386c",
        },
        mac: {
          label: "var(--mac-label)",
          secondary: "var(--mac-secondary)",
          tertiary: "var(--mac-tertiary)",
          bg: "var(--mac-bg)",
          window: "var(--mac-bg-window)",
          card: "var(--mac-bg-card)",
          fill: "var(--mac-fill)",
          separator: "var(--mac-separator)",
          blue: "var(--mac-blue)",
          green: "var(--mac-green-solid)",
          red: "var(--mac-red)",
          orange: "var(--mac-orange)",
          purple: "var(--mac-purple)",
        },
      },
      borderRadius: {
        mac: "var(--mac-radius-md)",
        "mac-lg": "var(--mac-radius-lg)",
        "mac-xl": "var(--mac-radius-xl)",
      },
      boxShadow: {
        mac: "var(--mac-shadow-md)",
        "mac-lg": "var(--mac-shadow-lg)",
      },
      fontFamily: {
        mac: "var(--mac-font)",
      },
    },
  },

  plugins: [flowbitePlugin],
};
