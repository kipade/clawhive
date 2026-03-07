import { create } from "zustand";

type Theme = "light" | "dark" | "system";

interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  const isDark =
    theme === "dark" ||
    (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  root.classList.toggle("dark", isDark);
}

export const useThemeStore = create<ThemeState>((set) => {
  const stored = (localStorage.getItem("clawhive-theme") as Theme) || "system";
  // Apply on store creation
  if (typeof document !== "undefined") {
    applyTheme(stored);
  }

  return {
    theme: stored,
    setTheme: (theme) => {
      localStorage.setItem("clawhive-theme", theme);
      applyTheme(theme);
      set({ theme });
    },
  };
});

// Listen for system preference changes
if (typeof window !== "undefined") {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    const currentTheme = useThemeStore.getState().theme;
    if (currentTheme === "system") {
      applyTheme("system");
    }
  });
}
