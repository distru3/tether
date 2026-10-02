/**
 * Pure derivations for the dashboard header. Kept out of `App.tsx` so the
 * component only wires data, and so each number has one documented meaning.
 */
import { dayKeyToDate } from "./format";
import i18n from "./i18n";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { DaySummaryDto } from "./types/generated/DaySummaryDto";
import type { WeeklySummaryDto } from "./types/generated/WeeklySummaryDto";
import type { LimitOutlook } from "./components/ExecutiveHeader";

/**
 * Which limits are used up, and which one is nearest.
 *
 * Per-app and per-category budgets come from the summary rows: the agent sets
 * `limit_seconds` so that `limit_seconds - seconds` is exactly the time left
 * on that row's own enabled limit (weekday overrides applied; for categories,
 * tagged apps counted). A granted "+15 min" extension is a wall-clock timer
 * (`timer_expires_utc`), not extra budget. The total-screen-time limit has no
 * row, so it is resolved from the catalog for the viewed weekday.
 */
export function limitOutlook(
    summary: DaySummaryDto | null,
    catalog: CatalogDto | null,
    viewDay: number,
): LimitOutlook {
    const candidates: Array<{ label: string; used: number; budget: number }> = [];
    for (const row of [...(summary?.apps ?? []), ...(summary?.categories ?? [])]) {
        if (row.limit_seconds !== null) {
            if (row.timer_expires_utc !== null) continue; // extended right now
            candidates.push({ label: row.label, used: row.seconds, budget: row.limit_seconds });
        }
    }
    const totalLimit = catalog?.limits.find((l) => l.enabled && l.target.kind === "total");
    if (totalLimit) {
        const mondayFirst = (dayKeyToDate(viewDay).getDay() + 6) % 7;
        const minutes = totalLimit.weekday_minutes[mondayFirst] ?? totalLimit.default_minutes;
        candidates.push({
            label: i18n.t("limitEditor.totalScreenTime", "Total screen time"),
            used: summary?.total_seconds ?? 0,
            budget: minutes * 60,
        });
    }

    const reached = candidates.filter((c) => c.used >= c.budget).map((c) => c.label);
    const open = candidates
        .filter((c) => c.used < c.budget)
        .sort((a, b) => a.budget - a.used - (b.budget - b.used));
    const nearest = open[0];
    return {
        reached,
        closest: nearest ? { label: nearest.label, remainingSeconds: nearest.budget - nearest.used } : null,
    };
}

/** The single longest uninterrupted interval of the day, and its app. */
export function longestStretch(summary: DaySummaryDto | null): { seconds: number; app: string | null } {
    let best = { seconds: 0, appId: 0 };
    for (const interval of summary?.intervals ?? []) {
        if (interval.durationSeconds > best.seconds) {
            best = { seconds: interval.durationSeconds, appId: interval.appId };
        }
    }
    const app = summary?.apps.find((a) => a.id === best.appId)?.label ?? null;
    return { seconds: best.seconds, app };
}

/**
 * Percent difference of `total` against the average of the other active days
 * this week, or null while there is no baseline (fewer than two active days).
 */
export function vsWeekAverage(total: number, week: WeeklySummaryDto | null, viewDay: number): number | null {
    const others = (week?.days ?? []).filter((d) => d.day !== viewDay && d.total_seconds > 0);
    if (others.length < 1 || (week?.days ?? []).filter((d) => d.total_seconds > 0).length < 2) return null;
    const average = others.reduce((sum, d) => sum + d.total_seconds, 0) / others.length;
    if (average <= 0) return null;
    return Math.round(((total - average) / average) * 100);
}
