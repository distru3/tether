import i18n from "./i18n";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { LimitTargetDto } from "./types/generated/LimitTargetDto";

export function todayKey(now = new Date()): number {
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

/**
 * Split a duration into display parts: `[["6", "h"], ["11", "m"]]`.
 *
 * Durations are written as amounts ("6h 11m"), never as clock faces
 * ("6:11:30"): a clock-style total reads like a time of day, and the old
 * mixture of `h:mm:ss` and `mm:ss` made "39:33" ambiguous. Seconds only appear
 * for spans under a minute. Units are localized.
 */
export function durationParts(totalSeconds: number): Array<[string, string]> {
    const s = Math.max(0, Math.round(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const unit = (key: "h" | "m" | "s") => i18n.t(`time.short.${key}`, key);
    if (h > 0) {
        return m > 0 ? [[String(h), unit("h")], [String(m), unit("m")]] : [[String(h), unit("h")]];
    }
    if (m > 0 || s === 0) return [[String(m), unit("m")]];
    return [[String(s), unit("s")]];
}

/** "6h 11m", "44m", "35s". See [`durationParts`]. */
export function formatDuration(totalSeconds: number): string {
    return durationParts(totalSeconds)
        .map(([value, unit]) => `${value}${unit}`)
        .join(" ");
}

/** The hero readout: the same parts, rendered with a smaller unit. */
export function heroParts(totalSeconds: number): Array<[string, string]> {
    return durationParts(totalSeconds);
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
            return i18n.t("limitEditor.totalScreenTime", "Total screen time");
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

/** Monday-first short weekday names in the UI language ("Mon" / "الاثنين"). */
export function weekdayShortNames(): string[] {
    const fmt = new Intl.DateTimeFormat(i18n.language, { weekday: "short" });
    // 2024-01-01 was a Monday.
    return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2024, 0, 1 + i, 12)));
}

/** e.g. "Sat 120m · Sun 45m", or null when no day overrides its default. */
export function describeWeekdayOverrides(weekdays: readonly (number | null)[]): string | null {
    const parts: string[] = [];
    for (let i = 0; i < WEEKDAY_SHORT.length && i < weekdays.length; i += 1) {
        const minutes = weekdays[i];
        if (minutes !== null && minutes !== undefined) parts.push(`${weekdayShortNames()[i]} ${formatDuration(minutes * 60)}`);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
}
