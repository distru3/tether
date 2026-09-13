import { useEffect, useState } from "react";

export type ThemePreference = "dark" | "light" | "system";
export type EffectiveTheme = "dark" | "light";

const STORAGE_KEY = "tether_theme";
const DEFAULT: ThemePreference = "system";

function getStored(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "dark" || v === "light" || v === "system") return v;
  } catch {
    // ignore
  }
  return DEFAULT;
}

function resolveEffective(pref: ThemePreference): EffectiveTheme {
  if (pref === "dark") return "dark";
  if (pref === "light") return "light";
  // system: read OS preference
  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  } catch {
    return "dark";
  }
}

function applyTheme(effective: EffectiveTheme) {
  document.documentElement.setAttribute("data-theme", effective);
}

export function useTheme() {
  const [theme, setThemeState] = useState<ThemePreference>(getStored);
  const [effectiveTheme, setEffectiveTheme] = useState<EffectiveTheme>(() =>
    resolveEffective(getStored())
  );

  // Apply and persist whenever the preference changes
  useEffect(() => {
    const effective = resolveEffective(theme);
    setEffectiveTheme(effective);
    applyTheme(effective);
    try {
      localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // ignore
    }
  }, [theme]);

  // For "system" mode: watch OS preference changes in real time
  useEffect(() => {
    if (theme !== "system") return;

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => {
      const effective: EffectiveTheme = e.matches ? "dark" : "light";
      setEffectiveTheme(effective);
      applyTheme(effective);
    };

    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [theme]);

  function setTheme(next: ThemePreference) {
    setThemeState(next);
  }

  return { theme, setTheme, effectiveTheme };
}
