import { flushSync } from "react-dom";

/** Which way a navigation moves: deeper/rightwards, or back/leftwards. */
export type NavDirection = "forward" | "back";

export function prefersReducedMotion(): boolean {
    return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Change what the main area shows with a directional page transition
 * (`::view-transition-*(page)` in tether.css). The update runs inside
 * `flushSync` so the new state is painted before the browser captures it.
 * Falls back to an instant update without the View Transitions API or with
 * reduced motion.
 */
export function navigate(direction: NavDirection, update: () => void): void {
    if (typeof document.startViewTransition !== "function" || prefersReducedMotion()) {
        update();
        return;
    }
    document.documentElement.dataset.navDir = direction;
    document.startViewTransition(() => flushSync(update));
}
