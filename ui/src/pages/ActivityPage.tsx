import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronLeft, ChevronRight } from "lucide-react";
import i18n from "../i18n";
import { getDaySummary, getWeeklySummary } from "../api";
import type { BudgetHue } from "../budgetHue";
import { dayKeyToDate, formatDuration, shiftDay, targetLabel, weekdayShortNames } from "../format";
import {
    budgetWeek,
    chartMaxSeconds,
    dayByHue,
    STACK_ORDER,
    totalBudgetOn,
    weekDays,
    weekTopApps,
} from "../activityModel";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";

interface ActivityPageProps {
    catalog: CatalogDto | null;
    /** The agent's today (day key). */
    today: number;
    now: Date;
    /** Open a day on Today. */
    onOpenDay: (day: number) => void;
}

function shortDate(day: number): string {
    return dayKeyToDate(day).toLocaleDateString(i18n.language, { day: "numeric", month: "short" });
}

/**
 * Activity (docs/DESIGN_SYSTEM.md §7): the week against the limits. Loads
 * the weekly totals plus each day's summary, so the bars can be split by
 * budget the same way the day strip on Today is.
 */
export function ActivityPage({ catalog, today, now, onOpenDay }: ActivityPageProps) {
    const { t } = useTranslation();
    const [endDay, setEndDay] = useState(today);
    const [week, setWeek] = useState<WeeklySummaryDto | null>(null);
    const [summaries, setSummaries] = useState<Map<number, DaySummaryDto>>(new Map());
    const [loading, setLoading] = useState(true);

    // Follow the day rollover while showing the current week.
    useEffect(() => {
        setEndDay((prev) => (prev > today ? today : prev));
    }, [today]);

    const days = useMemo(() => weekDays(endDay), [endDay]);

    useEffect(() => {
        let live = true;
        setLoading(true);
        (async () => {
            const [w, ...daySummaries] = await Promise.all([
                getWeeklySummary(endDay).catch(() => null),
                ...days.map((d) => getDaySummary(d).catch(() => null)),
            ]);
            if (!live) return;
            const map = new Map<number, DaySummaryDto>();
            daySummaries.forEach((s, i) => {
                const day = days[i];
                if (s && day !== undefined) map.set(day, s);
            });
            setWeek(w);
            setSummaries(map);
            setLoading(false);
        })();
        return () => {
            live = false;
        };
    }, [endDay, days]);

    const totals = days.map((d) => week?.days.find((x) => x.day === d)?.total_seconds ?? summaries.get(d)?.total_seconds ?? 0);
    const weekTotal = totals.reduce((a, b) => a + b, 0);
    const prev = week?.previous_week_total ?? 0;
    const max = chartMaxSeconds([...totals, ...days.map((d) => totalBudgetOn(catalog, d) ?? 0)]);
    const budgets = budgetWeek(catalog, summaries, days, now);
    const apps = weekTopApps(summaries);
    const names = weekdayShortNames();
    const lineBudget = totalBudgetOn(catalog, endDay);
    const pastDays = days.filter((d) => d <= today);
    const underDays = pastDays.filter((d, i) => {
        const b = totalBudgetOn(catalog, d);
        return b !== null && (totals[i] ?? 0) <= b;
    }).length;

    let summaryText = t("activity.weekTotal", { amount: formatDuration(weekTotal) });
    if (prev > 0 && weekTotal !== prev) {
        summaryText +=
            ", " +
            (weekTotal < prev
                ? t("activity.lessThanLast", { amount: formatDuration(prev - weekTotal) })
                : t("activity.moreThanLast", { amount: formatDuration(weekTotal - prev) }));
    }
    summaryText += ".";
    if (lineBudget !== null && pastDays.length > 0) {
        summaryText += " " + t("activity.stayedUnder", { budget: formatDuration(lineBudget), count: underDays, days: pastDays.length });
    }

    const hueName = (hue: BudgetHue) => {
        if (hue === "other") return t("today.everythingElse");
        const named = budgets.filter((b) => b.hue === hue).map((b) => targetLabel(b.limit.target, catalog));
        return named.length > 0 ? named.join(", ") : t("today.everythingElse");
    };
    const huesUsed = STACK_ORDER.filter((h) => days.some((d) => dayByHue(summaries.get(d), catalog)[h] > 0));
    const ticks = Array.from({ length: max / 3600 + 1 }, (_, i) => i * 3600).filter(
        (s, i, all) => all.length <= 7 || i % 2 === 0 || s === max,
    );
    const chartLabel = days
        .map((d, i) => `${names[(dayKeyToDate(d).getDay() + 6) % 7]} ${formatDuration(totals[i] ?? 0)}`)
        .join(", ");

    return (
        <div className="tt-page tt-activity">
            <header className="tt-head">
                <div>
                    <h1 className="tt-title">{t("activity.title")}</h1>
                    <p className="tt-sub">{loading && !week ? t("limitsPage.loading") : summaryText}</p>
                </div>
                <div className="tt-week-stepper">
                    <button type="button" onClick={() => setEndDay(shiftDay(endDay, -7))} aria-label={t("activity.prevWeek")} title={t("activity.prevWeek")}>
                        <ChevronLeft size={16} aria-hidden="true" />
                    </button>
                    <span>
                        {shortDate(days[0] ?? endDay)} – {shortDate(endDay)}
                    </span>
                    <button
                        type="button"
                        onClick={() => setEndDay(Math.min(today, shiftDay(endDay, 7)))}
                        disabled={endDay >= today}
                        aria-label={t("activity.nextWeek")}
                        title={t("activity.nextWeek")}
                    >
                        <ChevronRight size={16} aria-hidden="true" />
                    </button>
                </div>
            </header>

            <div className="tt-columns">
                <section className="tt-card tt-col-main" aria-labelledby="act-week">
                    <h2 id="act-week" className="tt-card-title">{t("activity.byDay")}</h2>
                    <div className="tt-act-chart" role="group" aria-labelledby="act-week" aria-describedby="act-week-summary">
                        <div className="tt-act-axis" aria-hidden="true">
                            {ticks.map((s) => (
                                <span key={s} style={{ bottom: `${(s / max) * 100}%` }}>
                                    {s === 0 ? "0" : formatDuration(s)}
                                </span>
                            ))}
                        </div>
                        <div className="tt-act-plot">
                            {ticks.map((s) => (
                                <span key={s} className="tt-act-grid" style={{ bottom: `${(s / max) * 100}%` }} aria-hidden="true" />
                            ))}
                            {lineBudget !== null && (
                                <span className="tt-act-limit" style={{ bottom: `${(lineBudget / max) * 100}%` }} aria-hidden="true" />
                            )}
                            {days.map((d, i) => {
                                const parts = dayByHue(summaries.get(d), catalog);
                                const total = totals[i] ?? 0;
                                // Usage outside any app row (rare) goes to "other".
                                const counted = STACK_ORDER.reduce((a, h) => a + parts[h], 0);
                                if (total > counted) parts.other += total - counted;
                                return (
                                    <button
                                        type="button"
                                        key={d}
                                        className={`tt-act-bar${d === today ? " tt-act-bar--today" : ""}`}
                                        onClick={() => onOpenDay(d)}
                                        disabled={d > today}
                                        aria-label={t("activity.openDay", { day: shortDate(d), amount: formatDuration(total) })}
                                    >
                                        {STACK_ORDER.map((h) =>
                                            parts[h] > 0 ? (
                                                <span key={h} className={`tt-hue-${h}`} style={{ height: `${(parts[h] / max) * 100}%` }} />
                                            ) : null,
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                    <p id="act-week-summary" className="tt-sr-only">{chartLabel}</p>
                    <div className="tt-act-days" aria-hidden="true">
                        {days.map((d, i) => {
                            const budget = totalBudgetOn(catalog, d);
                            const over = budget !== null && (totals[i] ?? 0) > budget;
                            return (
                                <span key={d} className={`${over ? "tt-act-day--over" : ""}${d === today ? " tt-act-day--today" : ""}`}>
                                    {d === today ? t("sidebar.today") : names[(dayKeyToDate(d).getDay() + 6) % 7]}
                                    <br />
                                    {formatDuration(totals[i] ?? 0)}
                                </span>
                            );
                        })}
                    </div>
                    <ul className="tt-strip-legend">
                        {huesUsed.map((h) => (
                            <li key={h} className={`tt-hue-${h}`}>
                                <span className="tt-strip-key" aria-hidden="true" />
                                {hueName(h)}
                            </li>
                        ))}
                        {lineBudget !== null && (
                            <li>
                                <span className="tt-act-limit-key" aria-hidden="true" />
                                {t("activity.limitLine", { budget: formatDuration(lineBudget) })}
                            </li>
                        )}
                    </ul>
                </section>

                <div className="tt-col-side">
                    <section className="tt-card" aria-labelledby="act-budgets">
                        <h2 id="act-budgets" className="tt-card-title">{t("activity.budgetsTitle")}</h2>
                        {budgets.length === 0 ? (
                            <p className="tt-sub">{t("activity.noBudgets")}</p>
                        ) : (
                            <table className="tt-act-table">
                                <thead>
                                    <tr>
                                        <th scope="col">{t("activity.budget")}</th>
                                        <th scope="col">{t("activity.dailyAvg")}</th>
                                        <th scope="col">{t("activity.ranOut")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {budgets.map((b) => (
                                        <tr key={b.limit.id}>
                                            <th scope="row">
                                                <span className={`tt-strip-key tt-hue-${b.hue}`} aria-hidden="true" />
                                                {targetLabel(b.limit.target, catalog)}
                                            </th>
                                            <td>{formatDuration(b.avgSeconds)}</td>
                                            <td>{t("activity.days", { count: b.ranOut })}</td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        )}
                    </section>
                    <section className="tt-card" aria-labelledby="act-apps">
                        <h2 id="act-apps" className="tt-card-title">{t("activity.topApps")}</h2>
                        {apps.length === 0 ? (
                            <p className="tt-sub">{t("usage.empty")}</p>
                        ) : (
                            <ol className="tt-act-apps">
                                {apps.map((a) => (
                                    <li key={a.id}>
                                        <span>{a.label}</span>
                                        <span>{formatDuration(a.seconds)}</span>
                                    </li>
                                ))}
                            </ol>
                        )}
                    </section>
                </div>
            </div>
        </div>
    );
}
