/**
 * Pure derivations for the Activity page: a week of days split by budget,
 * how each budget did over the week, and the week's most used apps.
 * Callers pass `now` and the day summaries; nothing here reads the clock.
 */
import type { BudgetHue } from "./budgetHue";
import { shiftDay } from "./format";
import { budgetStates, hueForApp, minutesOn, weekdayOf } from "./todayModel";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { DaySummaryDto } from "./types/generated/DaySummaryDto";
import type { LimitDto } from "./types/generated/LimitDto";

/** The order budget colours stack in, bottom to top. */
export const STACK_ORDER: BudgetHue[] = ["other", "video", "social", "games"];

/** The seven day keys ending at `endDay`, oldest first. */
export function weekDays(endDay: number): number[] {
    return Array.from({ length: 7 }, (_, i) => shiftDay(endDay, i - 6));
}

/** Seconds of use per budget colour on one day. */
export function dayByHue(summary: DaySummaryDto | null | undefined, catalog: CatalogDto | null): Record<BudgetHue, number> {
    const out: Record<BudgetHue, number> = { games: 0, social: 0, video: 0, total: 0, other: 0 };
    for (const row of summary?.apps ?? []) {
        out[hueForApp(row.id, catalog).hue] += row.seconds;
    }
    return out;
}

/** The total-screen-time budget for a day, in seconds, if one is on. */
export function totalBudgetOn(catalog: CatalogDto | null, day: number): number | null {
    const limit = catalog?.limits.find((l) => l.enabled && l.target.kind === "total");
    return limit ? minutesOn(limit, weekdayOf(day)) * 60 : null;
}

export interface BudgetWeek {
    limit: LimitDto;
    hue: BudgetHue;
    /** Average use per day over the days that have a summary. */
    avgSeconds: number;
    /** Days on which nothing was left. */
    ranOut: number;
}

/**
 * How each enabled app or category budget did across the week. A day is
 * counted only when its summary loaded.
 */
export function budgetWeek(
    catalog: CatalogDto | null,
    summaries: ReadonlyMap<number, DaySummaryDto>,
    days: number[],
    now: Date,
): BudgetWeek[] {
    const acc = new Map<number, BudgetWeek & { n: number; used: number }>();
    for (const day of days) {
        const summary = summaries.get(day);
        if (!summary) continue;
        for (const state of budgetStates(catalog, summary, day, now)) {
            const row = acc.get(state.limit.id) ?? { limit: state.limit, hue: state.hue, avgSeconds: 0, ranOut: 0, n: 0, used: 0 };
            row.n += 1;
            row.used += state.used;
            if (state.left <= 0) row.ranOut += 1;
            acc.set(state.limit.id, row);
        }
    }
    return [...acc.values()].map(({ n, used, ...rest }) => ({ ...rest, avgSeconds: n > 0 ? Math.round(used / n) : 0 }));
}

/** The week's most used apps, summed across the loaded days. */
export function weekTopApps(
    summaries: ReadonlyMap<number, DaySummaryDto>,
    limit = 5,
): Array<{ id: number; label: string; seconds: number }> {
    const totals = new Map<number, { id: number; label: string; seconds: number }>();
    for (const summary of summaries.values()) {
        for (const row of summary.apps) {
            const t = totals.get(row.id) ?? { id: row.id, label: row.label, seconds: 0 };
            t.seconds += row.seconds;
            totals.set(row.id, t);
        }
    }
    return [...totals.values()].filter((t) => t.seconds > 0).sort((a, b) => b.seconds - a.seconds).slice(0, limit);
}

/** A tidy top for the chart's scale: the next whole hour above the data. */
export function chartMaxSeconds(values: number[]): number {
    const max = Math.max(3600, ...values);
    return Math.ceil(max / 3600) * 3600;
}

export interface BudgetSuggestion {
    appId: number;
    label: string;
    /** Average use per loaded day, seconds. */
    avgSeconds: number;
}

/** Average a day's use must reach before an app is worth a suggestion. */
export const SUGGEST_MIN_AVG_SECONDS = 45 * 60;

/**
 * Today's "from last week" card: the most used app that no budget covers,
 * if it averaged at least 45 minutes a day. Apps in a category that can't
 * take a limit (`block_only`, `never_block`) are skipped. Adding a budget
 * only tightens, so the card never needs the PIN.
 */
export function suggestBudget(
    catalog: CatalogDto | null,
    summaries: ReadonlyMap<number, DaySummaryDto>,
    minAvgSeconds = SUGGEST_MIN_AVG_SECONDS,
): BudgetSuggestion | null {
    if (!catalog || summaries.size === 0) return null;
    const totals = new Map<number, { label: string; seconds: number }>();
    for (const summary of summaries.values()) {
        for (const row of summary.apps) {
            const t = totals.get(row.id) ?? { label: row.label, seconds: 0 };
            t.seconds += row.seconds;
            totals.set(row.id, t);
        }
    }
    let best: BudgetSuggestion | null = null;
    for (const [appId, { label, seconds }] of totals) {
        const avgSeconds = Math.round(seconds / summaries.size);
        if (avgSeconds < minAvgSeconds || (best && avgSeconds <= best.avgSeconds)) continue;
        const app = catalog.apps.find((a) => a.id === appId);
        if (!app) continue;
        const kind = catalog.categories.find((c) => c.id === app.primary_category)?.kind;
        if (kind !== undefined && kind !== "limitable") continue;
        if (hueForApp(appId, catalog).limit !== null) continue;
        // A switched-off budget of its own: the person already decided.
        if (catalog.limits.some((l) => l.target.kind === "app" && l.target.id === appId)) continue;
        best = { appId, label, avgSeconds };
    }
    return best;
}
