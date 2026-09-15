import { useEffect, useState } from "react";

export type ThemePreference =
  | "horizon-dark"
  | "horizon-light"
  | "classic-dark"
  | "classic-light"
  | "system";

export type EffectiveTheme =
  | "horizon-dark"
  | "horizon-light"
  | "classic-dark"
  | "classic-light";

const STORAGE_KEY = "tether_theme";
const DEFAULT: ThemePreference = "system";

// ---------------------------------------------------------------------------
// Storage helpers — Tauri file store (primary) + localStorage (fallback)
// ---------------------------------------------------------------------------

function isTauriAvailable(): boolean {
  return (
    typeof (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !==
    "undefined"
  );
}

function isValidPref(v: string | null | undefined): v is ThemePreference {
  return (
    v === "horizon-dark" ||
    v === "horizon-light" ||
    v === "classic-dark" ||
    v === "classic-light" ||
    v === "system"
  );
}

/** Read the saved theme synchronously from localStorage — used for the initial
 *  React state to avoid a blank frame before the async Tauri call resolves. */
function getStoredSync(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (isValidPref(v)) return v;
    // Backward compat for legacy "dark" / "light" values
    if (v === "dark") return "horizon-dark";
    if (v === "light") return "horizon-light";
  } catch {
    // ignore
  }
  return DEFAULT;
}

/** Read the authoritative saved theme from the Tauri backend (async).
 *  Falls back to localStorage if Tauri is unavailable. */
async function getStoredAsync(): Promise<ThemePreference> {
  if (!isTauriAvailable()) return getStoredSync();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const v = await invoke<string>("get_theme");
    if (isValidPref(v)) return v;
  } catch {
    // fall back to localStorage
  }
  return getStoredSync();
}

/** Persist the theme to both the Tauri file store and localStorage.
 *  - localStorage keeps index.html inline script working (prevents FOUC).
 *  - Tauri file store survives forced process kills in dev. */
async function persistTheme(theme: ThemePreference): Promise<void> {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // ignore
  }
  if (!isTauriAvailable()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_theme", { theme });
  } catch {
    // ignore — localStorage already updated
  }
}

// ---------------------------------------------------------------------------
// Theme resolution & DOM application
// ---------------------------------------------------------------------------

function resolveEffective(pref: ThemePreference): EffectiveTheme {
  if (pref === "horizon-dark") return "horizon-dark";
  if (pref === "horizon-light") return "horizon-light";
  if (pref === "classic-dark") return "classic-dark";
  if (pref === "classic-light") return "classic-light";
  // "system": follow the OS dark/light mode preference
  try {
    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    return isDark ? "horizon-dark" : "horizon-light";
  } catch {
    return "horizon-dark";
  }
}

function applyTheme(effective: EffectiveTheme) {
  document.documentElement.setAttribute("data-theme", effective);
  const mode = effective.includes("light") ? "light" : "dark";
  document.documentElement.setAttribute("data-theme-mode", mode);
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useTheme() {
  const [theme, setThemeState] = useState<ThemePreference>(getStoredSync);
  const [effectiveTheme, setEffectiveTheme] = useState<EffectiveTheme>(() =>
    resolveEffective(getStoredSync())
  );

  // On mount: load the authoritative value from the Tauri file store.
  // This corrects any mismatch between localStorage (which may be stale after
  // a forced kill in dev) and the file-backed store.
  useEffect(() => {
    getStoredAsync().then((saved) => {
      if (saved !== theme) {
        setThemeState(saved);
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // intentionally runs only once on mount

  // Apply and persist whenever the preference changes.
  useEffect(() => {
    const effective = resolveEffective(theme);
    setEffectiveTheme(effective);
    applyTheme(effective);
    void persistTheme(theme);
  }, [theme]);

  // For "system" mode: follow OS preference changes in real time.
  useEffect(() => {
    if (theme !== "system") return;

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => {
      const effective: EffectiveTheme = e.matches ? "horizon-dark" : "horizon-light";
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
