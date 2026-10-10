import type { LimitDto } from "./types/generated/LimitDto";

/**
 * Whether saving these values raises any day's limit, so the change waits
 * out the cooldown. Mirrors `is_loosening` in
 * `crates/agent/src/ipc_server/limits.rs`; the agent stays the authority and
 * reports the real effective time after saving. A new limit, or switching
 * one off, applies right away.
 */
export function waitsForCooldown(
    existing: LimitDto | null,
    defaultMinutes: number,
    weekdayMinutes: readonly (number | null)[],
    enabled: boolean,
): boolean {
    if (existing === null || !enabled) return false;
    if (defaultMinutes > existing.default_minutes) return true;
    return weekdayMinutes.some((next, i) => {
        const newEffective = next ?? defaultMinutes;
        const oldEffective = existing.weekday_minutes[i] ?? existing.default_minutes;
        return newEffective > oldEffective;
    });
}
