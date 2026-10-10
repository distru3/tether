/**
 * How limits and schedules read in words, shared by Today and Limits.
 */
import i18n from "./i18n";
import { formatDuration, weekdayShortNames } from "./format";
import type { LimitDto } from "./types/generated/LimitDto";

/** "10 pm" / "22:00", following the UI language. */
export function clockLabel(minute: number): string {
    const d = new Date(2024, 0, 1, Math.floor(minute / 60), minute % 60);
    return d.toLocaleTimeString(i18n.language, { hour: "numeric", minute: minute % 60 === 0 ? undefined : "2-digit" });
}

/**
 * A "back at …" time: "midnight" rather than "12 AM", which reads like noon
 * to many people. Other times as `clockLabel`.
 */
export function backAtLabel(d: Date): string {
    const minute = d.getHours() * 60 + d.getMinutes();
    return minute === 0 ? i18n.t("time.midnight") : clockLabel(minute);
}

/** Which days a schedule runs (Monday-first bit mask). */
export function daysLabel(mask: number): string {
    if ((mask & 127) === 127) return i18n.t("limitsPage.everyDay");
    if ((mask & 127) === 31) return i18n.t("limitsPage.weekdays");
    if ((mask & 127) === 96) return i18n.t("limitsPage.weekends");
    const names = weekdayShortNames();
    return names.filter((_, i) => (mask & (1 << i)) !== 0).join(" · ");
}

/** How a limit is set, in words: "1h every day", "1h · Sat 2h · Sun 2h". */
export function limitRule(limit: LimitDto): string {
    const base = formatDuration(limit.default_minutes * 60);
    const names = weekdayShortNames();
    const overrides = limit.weekday_minutes
        .map((m, i) => (m === null ? null : `${names[i]} ${formatDuration(m * 60)}`))
        .filter((x): x is string => x !== null);
    return overrides.length === 0
        ? i18n.t("limitsPage.everyDayAmount", { amount: base })
        : [base, ...overrides].join(" · ");
}
