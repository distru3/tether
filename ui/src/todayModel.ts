/**
 * Pure derivations for the Today screen (docs/DESIGN_SYSTEM.md §6): the ring,
 * the day strip and the budget tiles. No React, no clock reads: callers pass
 * `now`, so the numbers are testable and the screen re-renders each minute.
 */
import { hueFor, type BudgetHue } from "./budgetHue";
import { dayKeyToDate, shiftDay } from "./format";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { DaySummaryDto } from "./types/generated/DaySummaryDto";
import type { LimitDto } from "./types/generated/LimitDto";
import type { ScheduleDto } from "./types/generated/ScheduleDto";

const DAY_MS = 86_400_000;

/** At or under this many seconds left, a budget is "running low". */
export const LOW_SECONDS = 15 * 60;
/** ...or at or under this share of the day's budget. */
export const LOW_SHARE = 0.2;

/** Monday-first weekday (0 = Monday) of a day key. */
export function weekdayOf(dayKey: number): number {
    return (dayKeyToDate(dayKey).getDay() + 6) % 7;
}

/** The limit's minutes on that weekday (override, else the default). */
export function minutesOn(limit: LimitDto, weekday: number): number {
    return limit.weekday_minutes[weekday] ?? limit.default_minutes;
}

/** Local start of the agent's day `dayKey` (midnight + `dayStartMinutes`). */
export function dayWindowStart(dayKey: number, dayStartMinutes: number): Date {
    const d = dayKeyToDate(dayKey);
    d.setHours(0, dayStartMinutes, 0, 0);
    return d;
}

/** Position of `t` along the day window, 0..1 (unclamped). */
function along(t: number, start: number): number {
    return (t - start) / DAY_MS;
}

// ---------------------------------------------------------------- budgets

export type BudgetStatus = "plenty" | "low" | "done" | "extra";

export interface BudgetState {
    limit: LimitDto;
    hue: BudgetHue;
    /** Today's budget (weekday-resolved), seconds. */
    budget: number;
    /** Time left, seconds, never below 0. */
    left: number;
    used: number;
    status: BudgetStatus;
    /** When status is "extra": the extension's end. */
    extraUntil: Date | null;
    /** Apps that counted toward it on this day, most used first (max 3). */
    apps: string[];
}

function statusFor(left: number, budget: number): BudgetStatus {
    if (left <= 0) return "done";
    if (left <= LOW_SECONDS || (budget > 0 && left / budget <= LOW_SHARE)) return "low";
    return "plenty";
}

/** Apps that belong to a category limit: primary category or a tag. */
function inCategory(catalog: CatalogDto | null, appId: number, categoryId: number): boolean {
    const app = catalog?.apps.find((a) => a.id === appId);
    return app !== undefined && (app.primary_category === categoryId || app.tags.includes(categoryId));
}

/**
 * One tile per enabled app or category limit, in the catalog's order. The
 * total-screen-time limit is the ring, not a tile.
 *
 * Time left uses the agent's own arithmetic: a usage row's `limit_seconds`
 * is set so that `limit_seconds - seconds` is exactly what is left (weekday
 * overrides applied, tagged apps counted). Without a row nothing was used.
 */
export function budgetStates(
    catalog: CatalogDto | null,
    summary: DaySummaryDto | null,
    viewDay: number,
    now: Date,
): BudgetState[] {
    if (!catalog) return [];
    const weekday = weekdayOf(viewDay);
    const out: BudgetState[] = [];
    for (const limit of catalog.limits) {
        if (!limit.enabled || limit.target.kind === "total") continue;
        const target = limit.target;
        const budget = minutesOn(limit, weekday) * 60;
        const rows = target.kind === "app" ? summary?.apps : summary?.categories;
        const row = rows?.find((r) => r.id === target.id);
        const left = Math.max(0, row && row.limit_seconds !== null ? row.limit_seconds - row.seconds : budget);
        const used = Math.max(0, budget - left);
        const expires = row?.timer_expires_utc ? new Date(row.timer_expires_utc) : null;
        const extended = expires !== null && expires.getTime() > now.getTime();
        const apps = (summary?.apps ?? [])
            .filter((a) => a.seconds > 0)
            .filter((a) => (target.kind === "app" ? a.id === target.id : inCategory(catalog, a.id, target.id)))
            .sort((a, b) => b.seconds - a.seconds)
            .slice(0, 3)
            .map((a) => a.label);
        out.push({
            limit,
            hue: hueFor(target, catalog),
            budget,
            left,
            used,
            status: extended ? "extra" : statusFor(left, budget),
            extraUntil: extended ? expires : null,
            apps,
        });
    }
    return out;
}

/** The ring: the total-screen-time budget, when one is on. */
export interface TotalState {
    budget: number;
    left: number;
    status: BudgetStatus;
}

export function totalState(catalog: CatalogDto | null, summary: DaySummaryDto | null, viewDay: number): TotalState | null {
    const limit = catalog?.limits.find((l) => l.enabled && l.target.kind === "total");
    if (!limit) return null;
    const budget = minutesOn(limit, weekdayOf(viewDay)) * 60;
    const left = Math.max(0, budget - (summary?.total_seconds ?? 0));
    return { budget, left, status: statusFor(left, budget) };
}

// -------------------------------------------------------------- day strip

/** Which budget's colour an app's use takes on the strip. */
export function hueForApp(appId: number, catalog: CatalogDto | null): { hue: BudgetHue; limit: LimitDto | null } {
    const limits = (catalog?.limits ?? []).filter((l) => l.enabled);
    const own = limits.find((l) => l.target.kind === "app" && l.target.id === appId);
    if (own) return { hue: hueFor(own.target, catalog), limit: own };
    const cat = limits.find((l) => l.target.kind === "category" && inCategory(catalog, appId, l.target.id));
    if (cat) return { hue: hueFor(cat.target, catalog), limit: cat };
    return { hue: "other", limit: null };
}

export interface StripSegment {
    /** 0..1 along the day window. */
    from: number;
    to: number;
    hue: BudgetHue;
    appId: number;
    seconds: number;
}

/** Usage intervals placed on the day window, clipped to it. */
export function stripSegments(
    summary: DaySummaryDto | null,
    catalog: CatalogDto | null,
    viewDay: number,
    dayStartMinutes: number,
): StripSegment[] {
    const start = dayWindowStart(viewDay, dayStartMinutes).getTime();
    const out: StripSegment[] = [];
    for (const iv of summary?.intervals ?? []) {
        const t0 = new Date(iv.startUtc).getTime();
        const from = Math.max(0, along(t0, start));
        const to = Math.min(1, along(t0 + iv.durationSeconds * 1000, start));
        if (!(to > from)) continue;
        out.push({ from, to, hue: hueForApp(iv.appId, catalog).hue, appId: iv.appId, seconds: iv.durationSeconds });
    }
    return mergeSegments(out);
}

/** Gaps shorter than this (as a share of the day) join two same-hue blocks. */
const MERGE_GAP = 90 / 86_400;

/**
 * Join back-to-back intervals of the same colour. A day can hold thousands
 * of short intervals; at strip width they read as one block anyway.
 */
export function mergeSegments(segments: StripSegment[]): StripSegment[] {
    const sorted = [...segments].sort((a, b) => a.from - b.from);
    const out: StripSegment[] = [];
    for (const s of sorted) {
        const last = out[out.length - 1];
        if (last && last.hue === s.hue && s.from - last.to <= MERGE_GAP) {
            last.to = Math.max(last.to, s.to);
            last.seconds += s.seconds;
        } else {
            out.push({ ...s });
        }
    }
    return out;
}

export interface ScheduleBand {
    from: number;
    to: number;
    schedule: ScheduleDto;
    start: Date;
    end: Date;
}

/**
 * Downtime windows that overlap the day window. An occurrence belongs to the
 * weekday it starts on, so an overnight 22:00-07:00 schedule set for Friday
 * runs Friday night into Saturday morning (as `st_core::schedules` does).
 */
export function scheduleBands(schedules: ScheduleDto[], viewDay: number, dayStartMinutes: number): ScheduleBand[] {
    const winStart = dayWindowStart(viewDay, dayStartMinutes).getTime();
    const winEnd = winStart + DAY_MS;
    const base = dayKeyToDate(viewDay);
    const out: ScheduleBand[] = [];
    for (const s of schedules) {
        if (!s.enabled || s.start_minute === s.end_minute) continue;
        for (let offset = -1; offset <= 1; offset++) {
            const date = new Date(base.getFullYear(), base.getMonth(), base.getDate() + offset);
            const weekday = (date.getDay() + 6) % 7;
            if ((s.weekday_mask & (1 << weekday)) === 0) continue;
            const start = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 0, s.start_minute);
            const endDayOffset = s.end_minute <= s.start_minute ? 1 : 0;
            const end = new Date(date.getFullYear(), date.getMonth(), date.getDate() + endDayOffset, 0, s.end_minute);
            const a = Math.max(start.getTime(), winStart);
            const b = Math.min(end.getTime(), winEnd);
            if (b <= a) continue;
            out.push({ from: along(a, winStart), to: along(b, winStart), schedule: s, start, end });
        }
    }
    return out.sort((x, y) => x.from - y.from);
}

/** The schedule in force at `now`, or the next one to start, within a day. */
export function scheduleOutlook(
    schedules: ScheduleDto[],
    viewDay: number,
    dayStartMinutes: number,
    now: Date,
): { active: ScheduleBand | null; next: ScheduleBand | null } {
    // Look at today's window and the next, so "starts at 10 pm" still shows
    // after the day's own bands are past.
    const bands = [
        ...scheduleBands(schedules, viewDay, dayStartMinutes),
        ...scheduleBands(schedules, shiftDay(viewDay, 1), dayStartMinutes),
    ];
    const t = now.getTime();
    const active = bands.find((b) => b.start.getTime() <= t && t < b.end.getTime()) ?? null;
    const next =
        bands
            .filter((b) => b.start.getTime() > t && b.start.getTime() - t < DAY_MS)
            .sort((a, b) => a.start.getTime() - b.start.getTime())[0] ?? null;
    return { active, next };
}

/** Where "now" sits on the day window (0..1), or null outside it. */
export function nowPosition(viewDay: number, dayStartMinutes: number, now: Date): number | null {
    const p = along(now.getTime(), dayWindowStart(viewDay, dayStartMinutes).getTime());
    return p >= 0 && p <= 1 ? p : null;
}
