import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Saved preference. "system" follows the OS light/dark setting. */
export type ThemePreference = "light" | "dark" | "system";

export type EffectiveTheme = "light" | "dark";

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
  if (v === "light" || v === "dark" || v === "system") return v;
  // Retired theme names (before the 2026-10 redesign) keep their light/dark
  // side. Mirrored in index.html and the Tauri `normalize_theme`.
  if (
    v === "midnight-cobalt" ||
    v === "slate-charcoal" ||
    v === "cyber-emerald" ||
    v === "horizon-dark" ||
    v === "classic-dark"
  ) {
    return "dark";
  }
  if (
    v === "clean-titanium" ||
    v === "nordic-frost" ||
    v === "horizon-light" ||
    v === "classic-light"
  ) {
    return "light";
  }
  return null;
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
  if (pref !== "system") return pref;
  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  } catch {
    return "dark";
  }
}

export function applyTheme(effective: EffectiveTheme) {
  document.documentElement.setAttribute("data-theme", effective);
  document.documentElement.setAttribute("data-theme-mode", effective);
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
