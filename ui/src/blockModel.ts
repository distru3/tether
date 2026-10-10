/**
 * Why an app is blocked, and when it comes back, for the block screen.
 *
 * The overlay bridge only says which app is blocked, so this re-derives the
 * reason in the agent's own order (`crates/agent/src/enforcer.rs`): a
 * downtime schedule in force blocks first unless the app is always allowed,
 * then the app's budget, then total screen time.
 */
import { budgetStates, dayWindowStart, hueForApp, scheduleOutlook, totalState, type ScheduleBand } from "./todayModel";
import type { BudgetHue } from "./budgetHue";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { DaySummaryDto } from "./types/generated/DaySummaryDto";
import type { LimitDto } from "./types/generated/LimitDto";
import type { ScheduleDto } from "./types/generated/ScheduleDto";

export type BlockReason =
    | { kind: "downtime"; band: ScheduleBand; until: Date }
    | { kind: "budget"; limit: LimitDto; hue: BudgetHue; budget: number; until: Date }
    | { kind: "total"; limit: LimitDto; budget: number; until: Date }
    | { kind: "unknown"; until: Date };

export interface BlockInputs {
    appId: number;
    catalog: CatalogDto | null;
    summary: DaySummaryDto | null;
    schedules: ScheduleDto[];
    /** App ids on the downtime allowlist. */
    allowlisted: number[];
    dayKey: number;
    dayStartMinutes: number;
    now: Date;
}

/** When the agent's day rolls over (and every budget block lifts). */
export function nextReset(dayKey: number, dayStartMinutes: number): Date {
    const d = dayWindowStart(dayKey, dayStartMinutes);
    d.setDate(d.getDate() + 1);
    return d;
}

export function blockReason(input: BlockInputs): BlockReason {
    const { appId, catalog, summary, schedules, allowlisted, dayKey, dayStartMinutes, now } = input;
    const reset = nextReset(dayKey, dayStartMinutes);

    const { active } = scheduleOutlook(schedules, dayKey, dayStartMinutes, now);
    if (active && !allowlisted.includes(appId)) {
        return { kind: "downtime", band: active, until: active.end };
    }

    const { limit } = hueForApp(appId, catalog);
    if (limit) {
        const state = budgetStates(catalog, summary, dayKey, now).find((b) => b.limit.id === limit.id);
        if (state && state.status === "done") {
            return { kind: "budget", limit, hue: state.hue, budget: state.budget, until: reset };
        }
    }

    const total = totalState(catalog, summary, dayKey);
    const totalLimit = catalog?.limits.find((l) => l.enabled && l.target.kind === "total");
    if (total && totalLimit && total.status === "done") {
        return { kind: "total", limit: totalLimit, budget: total.budget, until: reset };
    }

    // The agent blocked it, but the data here does not say why (e.g. the
    // summary is a moment behind). Budgets come back at the day reset.
    if (limit) {
        const state = budgetStates(catalog, summary, dayKey, now).find((b) => b.limit.id === limit.id);
        return { kind: "budget", limit, hue: state?.hue ?? "other", budget: state?.budget ?? limit.default_minutes * 60, until: reset };
    }
    return { kind: "unknown", until: reset };
}
