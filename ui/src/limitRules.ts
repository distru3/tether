import type { LimitDto } from "./types/generated/LimitDto";
import type { StatusDto } from "./types/generated/StatusDto";

/**
 * Hours a raised limit waits before it applies. Mirrors `cooldown_hours` in
 * `crates/agent/src/ipc_server/limits.rs` (docs/adr/0006): for someone
 * looked after, with a PIN, the PIN is the safeguard and raises apply at once.
 */
export function cooldownHoursFor(status: StatusDto | null): number {
    if (status === null) return 24;
    if (status.profile === "guardian" && status.pin_configured) return 0;
    return Math.max(0, Number(status.limit_cooldown_hours));
}

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
