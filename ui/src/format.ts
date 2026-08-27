import type { CatalogDto } from "./types/generated/CatalogDto";
import type { LimitTargetDto } from "./types/generated/LimitTargetDto";

export function todayKey(now = new Date()): number {
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

export function formatDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    if (s >= 3600) {
        const h = Math.floor(s / 3600);
        const m = Math.round((s % 3600) / 60);
        return m >= 60 ? `${h + 1}h 00m` : `${h}h ${m}m`;
    }
    if (s >= 60) return `${Math.round(s / 60)}m`;
    return `${s}s`;
}

export function heroParts(totalSeconds: number): Array<[string, string]> {
    const s = Math.max(0, Math.round(totalSeconds));
    if (s >= 3600) {
        let h = Math.floor(s / 3600);
        let m = Math.round((s % 3600) / 60);
        if (m >= 60) {
            h += 1;
            m = 0;
        }
        return [
            [String(h), "h"],
            [pad2(m), "m"],
        ];
    }
    if (s >= 60) return [[String(Math.round(s / 60)), "m"]];
    return [[String(s), "s"]];
}

function pad2(n: number): string {
    return n < 10 ? `0${n}` : String(n);
}

export function sharePercent(seconds: number, total: number): number {
    if (total <= 0) return 0;
    return Math.min(100, Math.round((seconds / total) * 100));
}

const WEEKDAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"] as const;
const FULL_WEEKDAYS = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"] as const;
const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"] as const;

export function formatDateline(d: Date): string {
    return `${WEEKDAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

export function formatClock(d: Date): string {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function effectClause(effectiveUtc: string | null | undefined): string {
    if (!effectiveUtc) return " Applied.";
    const t = new Date(effectiveUtc);
    if (Number.isNaN(t.getTime())) return " Applied.";
    if (t.getTime() <= Date.now()) return " Applied.";
    return ` Takes effect ${formatClock(t)}.`;
}

export function targetLabel(target: LimitTargetDto, catalog: CatalogDto | null): string {
    switch (target.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog?.apps.find((a) => a.id === target.id)?.display_name ?? `App #${target.id}`;
        case "category":
            return catalog?.categories.find((c) => c.id === target.id)?.name ?? `Category #${target.id}`;
    }
}

export function dayKeyToDate(key: number): Date {
    return new Date(Math.floor(key / 10000), Math.floor((key % 10000) / 100) - 1, key % 100, 12);
}

/** Whole local days of ±delta from a day key. Noon-based so DST never skips it. */
export function shiftDay(key: number, delta: number): number {
    return todayKey(new Date(dayKeyToDate(key).getTime() + delta * 86400000));
}

/** Dateline above the hero when browsing history: "TUESDAY · AUG 24". */
export function formatDayLabel(key: number): string {
    const d = dayKeyToDate(key);
    return `${FULL_WEEKDAYS[d.getDay()]} · ${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/** Compact ledger bar label: "07·23". */
export function chartBarLabel(key: number): string {
    const d = dayKeyToDate(key);
    return `${pad2(d.getMonth() + 1)}·${pad2(d.getDate())}`;
}

/** Short labels for the Monday-first weekday slots used by limits. */
export const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;

/** e.g. "Sat 120m · Sun 45m", or null when no day overrides its default. */
export function describeWeekdayOverrides(weekdays: readonly (number | null)[]): string | null {
    const parts: string[] = [];
    for (let i = 0; i < WEEKDAY_SHORT.length && i < weekdays.length; i += 1) {
        const minutes = weekdays[i];
        if (minutes !== null && minutes !== undefined) parts.push(`${WEEKDAY_SHORT[i]} ${minutes}m`);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
}
