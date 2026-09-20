import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ThemePreference =
  | "midnight-cobalt"
  | "slate-charcoal"
  | "clean-titanium"
  | "nordic-frost"
  | "system";

export type EffectiveTheme =
  | "midnight-cobalt"
  | "slate-charcoal"
  | "clean-titanium"
  | "nordic-frost";

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

function normalizePref(v: string | null | undefined): ThemePreference | null {
  if (!v) return null;
  if (
    v === "midnight-cobalt" ||
    v === "slate-charcoal" ||
    v === "clean-titanium" ||
    v === "nordic-frost" ||
    v === "system"
  ) {
    return v;
  }
  // Backward compatibility for legacy values
  if (v === "cyber-emerald") return "slate-charcoal";
  if (v === "horizon-dark" || v === "classic-dark" || v === "dark") return "midnight-cobalt";
  if (v === "horizon-light" || v === "classic-light" || v === "light") return "clean-titanium";
  return null;
}

function isValidPref(v: string | null | undefined): v is ThemePreference {
  return normalizePref(v) !== null;
}

/** Read the saved theme synchronously from localStorage — used for the initial
 *  React state to avoid a blank frame before the async Tauri call resolves. */
function getStoredSync(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    const normalized = normalizePref(v);
    if (normalized) return normalized;
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
    const v = await invoke<string>("get_theme");
    const normalized = normalizePref(v);
    if (normalized) return normalized;
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
    await invoke("set_theme", { theme });
  } catch {
    // ignore — localStorage already updated
  }
}

// ---------------------------------------------------------------------------
// Theme resolution & DOM application
// ---------------------------------------------------------------------------

function resolveEffective(pref: ThemePreference): EffectiveTheme {
  if (pref === "midnight-cobalt") return "midnight-cobalt";
  if (pref === "slate-charcoal") return "slate-charcoal";
  if (pref === "clean-titanium") return "clean-titanium";
  if (pref === "nordic-frost") return "nordic-frost";
  // "system": follow the OS dark/light mode preference
  try {
    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    return isDark ? "midnight-cobalt" : "clean-titanium";
  } catch {
    return "midnight-cobalt";
  }
}

export function applyTheme(effective: EffectiveTheme) {
  document.documentElement.setAttribute("data-theme", effective);
  const mode = effective === "clean-titanium" || effective === "nordic-frost" ? "light" : "dark";
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
  useEffect(() => {
    getStoredAsync().then((saved) => {
      if (saved !== theme) {
        setThemeState(saved);
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Listen for broadcasted theme changes across all Tauri windows (main, overlay, etc.)
  useEffect(() => {
    let unlisten: Promise<() => void> | null = null;
    try {
      if (isTauriAvailable()) {
        unlisten = listen<string>("theme_changed", (event) => {
          const next = normalizePref(event.payload);
          if (next && next !== theme) {
            setThemeState(next);
          }
        });
      }
    } catch {
      // ignore
    }
    return () => {
      if (unlisten) unlisten.then((f) => f());
    };
  }, [theme]);

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
      const effective: EffectiveTheme = e.matches ? "midnight-cobalt" : "clean-titanium";
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
