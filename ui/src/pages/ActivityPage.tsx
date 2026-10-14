import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUpRight, ChevronDown, ChevronLeft, ChevronRight } from "lucide-react";
import i18n from "../i18n";
import { hueLabel, type BudgetHue } from "../budgetHue";
import { dayKeyToDate, formatDuration, shiftDay, targetLabel, weekdayShortNames } from "../format";
import {
    appWeekSeries,
    budgetWeek,
    chartMaxSeconds,
    dailyAverage,
    dayApps,
    totalWeek,
    dayByHue,
    STACK_ORDER,
    totalBudgetOn,
    weekTopApps,
} from "../activityModel";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { useWeekDetail } from "../hooks/useWeekDetail";

interface ActivityPageProps {
    catalog: CatalogDto | null;
    /** The agent's today (day key). */
    today: number;
    now: Date;
    /** Open a day on Today (the day panel's "Open on Today" link). */
    onOpenDay: (day: number) => void;
    /** The account on screen (null: this window's own); refetches on change. */
    viewedAccount?: number | null;
}

type HueParts = Record<BudgetHue, number>;

function shortDate(day: number): string {
    return dayKeyToDate(day).toLocaleDateString(i18n.language, { day: "numeric", month: "short" });
}

function longDate(day: number): string {
    return dayKeyToDate(day).toLocaleDateString(i18n.language, { weekday: "long", day: "numeric", month: "long" });
}

/** The hues of a day, largest first, without empty ones. */
function huesOf(parts: HueParts): Array<[BudgetHue, number]> {
    return STACK_ORDER.filter((h) => parts[h] > 0)
        .map((h): [BudgetHue, number] => [h, parts[h]])
        .sort((a, b) => b[1] - a[1]);
}

/**
 * Activity (docs/DESIGN_SYSTEM.md §7): the week against the limits. Loads
 * the weekly totals plus each day's summary, so the bars split by hue the
 * same way the day strip on Today does. Everything is explored in place:
 * a bar selects its day for the day panel, the legend isolates a hue, and
 * an app row opens that app's week.
 */
export function ActivityPage({ catalog, today, now, onOpenDay, viewedAccount = null }: ActivityPageProps) {
    const { t } = useTranslation();
    const [endDay, setEndDay] = useState(today);
    const { days, week, summaries, reasons, loading } = useWeekDetail(endDay, true, viewedAccount);
    const [selected, setSelected] = useState(today);
    const [hovered, setHovered] = useState<number | null>(null);
    const [hueFilter, setHueFilter] = useState<BudgetHue | null>(null);
    const [openApp, setOpenApp] = useState<number | null>(null);
    const barRefs = useRef<Array<HTMLButtonElement | null>>([]);

    // Follow the day rollover while showing the current week.
    useEffect(() => {
        setEndDay((prev) => (prev > today ? today : prev));
    }, [today]);

    // A new week selects its last day (never one in the future).
    useEffect(() => {
        setSelected(Math.min(endDay, today));
        setOpenApp(null);
    }, [endDay, today]);

    const totals = days.map((d) => week?.days.find((x) => x.day === d)?.total_seconds ?? summaries.get(d)?.total_seconds ?? 0);
    const weekTotal = totals.reduce((a, b) => a + b, 0);
    const prev = week?.previous_week_total ?? 0;
    const max = chartMaxSeconds([...totals, ...days.map((d) => totalBudgetOn(catalog, d) ?? 0)]);
    const budgets = budgetWeek(catalog, summaries, days, now);
    const total = totalWeek(catalog, days, totals, today);
    const apps = weekTopApps(summaries);
    const names = weekdayShortNames();
    const lineBudget = totalBudgetOn(catalog, endDay);
    const pastDays = days.filter((d) => d <= today);
    const underDays = pastDays.filter((d, i) => {
        const b = totalBudgetOn(catalog, d);
        return b !== null && (totals[i] ?? 0) <= b;
    }).length;
    const average = dailyAverage(totals.filter((_, i) => (days[i] ?? 0) <= today));
    const dayName = (d: number) => (d === today ? t("sidebar.today") : names[(dayKeyToDate(d).getDay() + 6) % 7]);

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

    // Each day's use per hue; time outside any app row (rare) goes to "other".
    const dayParts = days.map((d, i) => {
        const parts = dayByHue(summaries.get(d), catalog);
        const counted = STACK_ORDER.reduce((a, h) => a + parts[h], 0);
        if ((totals[i] ?? 0) > counted) parts.other += (totals[i] ?? 0) - counted;
        return parts;
    });
    const huesUsed = STACK_ORDER.filter((h) => dayParts.some((parts) => parts[h] > 0));
    const ticks = Array.from({ length: max / 3600 + 1 }, (_, i) => i * 3600).filter(
        (s, i, all) => all.length <= 7 || i % 2 === 0 || s === max,
    );
    const chartLabel = days.map((d, i) => `${dayName(d)} ${formatDuration(totals[i] ?? 0)}`).join(", ");

    const selectedIndex = Math.max(0, days.indexOf(selected));
    const selectable = (i: number) => (days[i] ?? Infinity) <= today;

    // Arrow keys move the selection between days, in reading order.
    const onBarKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
        const rtl = document.documentElement.dir === "rtl";
        const step = e.key === "ArrowRight" ? (rtl ? -1 : 1) : e.key === "ArrowLeft" ? (rtl ? 1 : -1) : e.key === "Home" ? -7 : e.key === "End" ? 7 : 0;
        if (step === 0) return;
        e.preventDefault();
        let next = Math.max(0, Math.min(days.length - 1, i + step));
        while (next > 0 && !selectable(next)) next -= 1;
        const day = days[next];
        if (day === undefined) return;
        setSelected(day);
        barRefs.current[next]?.focus();
    };

    const tipIndex = hovered !== null ? days.indexOf(hovered) : -1;

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
                <div className="tt-col-main tt-col-stack">
                    <section className="tt-card" aria-labelledby="act-week">
                        <div className="tt-card-head">
                            <h2 id="act-week" className="tt-card-title">{t("activity.byDay")}</h2>
                            {average !== null && (
                                <span className="tt-act-avg">{t("activity.dailyAvgValue", { amount: formatDuration(average) })}</span>
                            )}
                        </div>
                        <div
                            className={`tt-act-chart${hueFilter ? " tt-act-chart--filtered" : ""}`}
                            role="group"
                            aria-labelledby="act-week"
                            aria-describedby="act-week-summary"
                            key={endDay}
                        >
                            <div className="tt-act-axis" aria-hidden="true">
                                {ticks.map((s) => (
                                    <span key={s} style={{ bottom: `${(s / max) * 100}%` }}>
                                        {s === 0 ? "0" : formatDuration(s)}
                                    </span>
                                ))}
                            </div>
                            <div className="tt-act-plot" onMouseLeave={() => setHovered(null)}>
                                {ticks.map((s) => (
                                    <span key={s} className="tt-act-grid" style={{ bottom: `${(s / max) * 100}%` }} aria-hidden="true" />
                                ))}
                                {lineBudget !== null && (
                                    <span className="tt-act-limit" style={{ bottom: `${(lineBudget / max) * 100}%` }} aria-hidden="true" />
                                )}
                                {days.map((d, i) => {
                                    const parts = dayParts[i] ?? dayByHue(null, catalog);
                                    const dayTotal = totals[i] ?? 0;
                                    const isSelected = d === selected;
                                    return (
                                        <button
                                            type="button"
                                            key={d}
                                            ref={(el) => {
                                                barRefs.current[i] = el;
                                            }}
                                            className={`tt-act-bar${isSelected ? " tt-act-bar--selected" : ""}${d === today ? " tt-act-bar--today" : ""}`}
                                            style={{ ["--i" as string]: i }}
                                            onClick={() => setSelected(d)}
                                            onMouseEnter={() => setHovered(d)}
                                            onFocus={() => setHovered(d)}
                                            onBlur={() => setHovered(null)}
                                            onKeyDown={(e) => onBarKey(e, i)}
                                            disabled={d > today}
                                            tabIndex={i === selectedIndex ? 0 : -1}
                                            aria-pressed={isSelected}
                                            aria-label={t("activity.selectDay", { day: longDate(d), amount: formatDuration(dayTotal) })}
                                        >
                                            <span className="tt-act-stack">
                                                {STACK_ORDER.map((h) =>
                                                    parts[h] > 0 ? (
                                                        <span
                                                            key={h}
                                                            className={`tt-hue-${h}${hueFilter && hueFilter !== h ? " tt-act-seg--muted" : ""}`}
                                                            style={{ height: `${(parts[h] / max) * 100}%` }}
                                                        />
                                                    ) : null,
                                                )}
                                            </span>
                                        </button>
                                    );
                                })}
                                {tipIndex >= 0 && (totals[tipIndex] ?? 0) > 0 && (
                                    <BarTip
                                        index={tipIndex}
                                        heightPct={((totals[tipIndex] ?? 0) / max) * 100}
                                        title={longDate(days[tipIndex] ?? today)}
                                        total={totals[tipIndex] ?? 0}
                                        parts={dayParts[tipIndex] ?? dayByHue(null, catalog)}
                                    />
                                )}
                            </div>
                        </div>
                        <p id="act-week-summary" className="tt-sr-only">{chartLabel}</p>
                        <div className="tt-act-days" aria-hidden="true">
                            {days.map((d, i) => {
                                const budget = totalBudgetOn(catalog, d);
                                const over = budget !== null && (totals[i] ?? 0) > budget;
                                return (
                                    <span
                                        key={d}
                                        className={`${over ? "tt-act-day--over" : ""}${d === today ? " tt-act-day--today" : ""}${d === selected ? " tt-act-day--selected" : ""}`}
                                    >
                                        {dayName(d)}
                                        <br />
                                        {formatDuration(totals[i] ?? 0)}
                                    </span>
                                );
                            })}
                        </div>
                        <ul className="tt-strip-legend tt-act-legend">
                            {huesUsed.map((h) => (
                                <li key={h} className={`tt-hue-${h}`}>
                                    <button
                                        type="button"
                                        className={`tt-act-legend-btn${hueFilter === h ? " tt-act-legend-btn--on" : ""}${hueFilter && hueFilter !== h ? " tt-act-legend-btn--off" : ""}`}
                                        aria-pressed={hueFilter === h}
                                        title={hueFilter === h ? t("activity.showAll") : t("activity.showOnly", { name: hueLabel(h) })}
                                        onClick={() => setHueFilter(hueFilter === h ? null : h)}
                                    >
                                        <span className="tt-strip-key" aria-hidden="true" />
                                        {hueLabel(h)}
                                    </button>
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

                    <DayPanel
                        day={selected}
                        today={today}
                        total={totals[selectedIndex] ?? 0}
                        average={average}
                        parts={dayParts[selectedIndex] ?? dayByHue(null, catalog)}
                        apps={dayApps(summaries.get(selected), catalog)}
                        hueFilter={hueFilter}
                        onClearFilter={() => setHueFilter(null)}
                        onOpenDay={onOpenDay}
                        loading={loading && !summaries.has(selected)}
                    />
                </div>

                <div className="tt-col-side">
                    <section className="tt-card" aria-labelledby="act-apps">
                        <h2 id="act-apps" className="tt-card-title">{t("activity.topApps")}</h2>
                        {apps.length === 0 ? (
                            <p className="tt-sub">{t("usage.empty")}</p>
                        ) : (
                            <ol className="tt-act-apps">
                                {apps.map((a) => {
                                    const open = openApp === a.id;
                                    const series = appWeekSeries(summaries, days, a.id);
                                    const peak = Math.max(...series);
                                    const peakDay = days[series.indexOf(peak)];
                                    const hue = dayApps(summaries.get(peakDay ?? today), catalog).find((x) => x.id === a.id)?.hue ?? "other";
                                    return (
                                        <li key={a.id} className={`tt-hue-${hue}${open ? " tt-act-app--open" : ""}`}>
                                            <button
                                                type="button"
                                                className="tt-act-app"
                                                aria-expanded={open}
                                                aria-controls={`act-app-${a.id}`}
                                                onClick={() => setOpenApp(open ? null : a.id)}
                                            >
                                                <span className="tt-strip-key" aria-hidden="true" />
                                                <span className="tt-act-app-name">{a.label}</span>
                                                <span className="tt-act-app-time">{formatDuration(a.seconds)}</span>
                                                <ChevronDown size={15} className="tt-act-app-chevron" aria-hidden="true" />
                                            </button>
                                            <div className="tt-act-app-more" id={`act-app-${a.id}`} hidden={!open}>
                                                {open && (
                                                    <>
                                                        <div className="tt-act-spark" role="group" aria-label={t("activity.appWeek", { app: a.label })}>
                                                            {days.map((d, i) => (
                                                                <button
                                                                    type="button"
                                                                    key={d}
                                                                    className={`tt-act-spark-col${d === selected ? " tt-act-spark-col--selected" : ""}`}
                                                                    style={{ ["--i" as string]: i }}
                                                                    disabled={d > today}
                                                                    onClick={() => setSelected(d)}
                                                                    aria-label={t("activity.selectDay", { day: longDate(d), amount: formatDuration(series[i] ?? 0) })}
                                                                >
                                                                    <span className="tt-act-spark-bar">
                                                                        <span style={{ height: `${peak > 0 ? ((series[i] ?? 0) / peak) * 100 : 0}%` }} />
                                                                    </span>
                                                                    <span className="tt-act-spark-day" aria-hidden="true">
                                                                        {names[(dayKeyToDate(d).getDay() + 6) % 7]}
                                                                    </span>
                                                                </button>
                                                            ))}
                                                        </div>
                                                        {peakDay !== undefined && peak > 0 && (
                                                            <p className="tt-act-app-note">
                                                                {t("activity.peakDay", { day: longDate(peakDay), amount: formatDuration(peak) })}
                                                            </p>
                                                        )}
                                                    </>
                                                )}
                                            </div>
                                        </li>
                                    );
                                })}
                            </ol>
                        )}
                    </section>
                    <section className="tt-card" aria-labelledby="act-budgets">
                        <h2 id="act-budgets" className="tt-card-title">{t("activity.budgetsTitle")}</h2>
                        {budgets.length === 0 && total === null ? (
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
                                    {total && (
                                        <tr>
                                            <th scope="row">
                                                <span className="tt-strip-key tt-hue-total" aria-hidden="true" />
                                                {targetLabel(total.limit.target, catalog)}
                                            </th>
                                            <td>{formatDuration(total.avgSeconds)}</td>
                                            <td>{t("activity.days", { count: total.ranOut })}</td>
                                        </tr>
                                    )}
                                </tbody>
                            </table>
                        )}
                    </section>
                    <section className="tt-card" aria-labelledby="act-why">
                        <h2 id="act-why" className="tt-card-title">{t("activity.whyTitle")}</h2>
                        <p className="tt-sub">{t("activity.whyBody")}</p>
                        {reasons.length === 0 ? (
                            <p className="tt-sub">{t("activity.whyEmpty")}</p>
                        ) : (
                            <ul className="tt-act-why">
                                {reasons.map((r) => (
                                    <li key={r.reason}>
                                        <span>{t(`overlay.reason.${r.reason}`)}</span>
                                        <span className="tt-act-why-bar" aria-hidden="true">
                                            <span style={{ width: `${(r.count / Math.max(1, reasons[0]?.count ?? 1)) * 100}%` }} />
                                        </span>
                                        <span>{r.count}</span>
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>
                </div>
            </div>
        </div>
    );
}

/** The hover card over a bar: the day, its total and its biggest hues. */
function BarTip({ index, heightPct, title, total, parts }: { index: number; heightPct: number; title: string; total: number; parts: HueParts }) {
    return (
        <div
            className="tt-act-tip"
            // Beside the bar (towards the middle of the chart), level with its
            // top but never outside the plot.
            style={
                index < 4
                    ? { insetInlineStart: `calc(${((index + 1) / 7) * 100}% + 4px)`, top: `clamp(0px, calc(${100 - heightPct}% - 20px), calc(100% - 150px))` }
                    : { insetInlineEnd: `calc(${((7 - index) / 7) * 100}% + 4px)`, top: `clamp(0px, calc(${100 - heightPct}% - 20px), calc(100% - 150px))` }
            }
            aria-hidden="true"
        >
            <strong>{title}</strong>
            <span className="tt-act-tip-total">{formatDuration(total)}</span>
            <ul>
                {huesOf(parts)
                    .slice(0, 4)
                    .map(([h, secs]) => (
                        <li key={h} className={`tt-hue-${h}`}>
                            <span className="tt-strip-key" />
                            <span>{hueLabel(h)}</span>
                            <span>{formatDuration(secs)}</span>
                        </li>
                    ))}
            </ul>
        </div>
    );
}

/** The selected day: total, how it compares, its hues and its apps. */
function DayPanel({
    day,
    today,
    total,
    average,
    parts,
    apps,
    hueFilter,
    onClearFilter,
    onOpenDay,
    loading,
}: {
    day: number;
    today: number;
    total: number;
    average: number | null;
    parts: HueParts;
    apps: ReturnType<typeof dayApps>;
    hueFilter: BudgetHue | null;
    onClearFilter: () => void;
    onOpenDay: (day: number) => void;
    loading: boolean;
}) {
    const { t } = useTranslation();
    const hues = huesOf(parts);
    const shown = (hueFilter ? apps.filter((a) => a.hue === hueFilter) : apps).slice(0, 6);
    const top = shown[0]?.seconds ?? 1;
    const diff = average !== null ? total - average : 0;
    const compare =
        average === null || total === 0
            ? null
            : Math.abs(diff) < 5 * 60
              ? t("activity.aboutAverage")
              : diff > 0
                ? t("activity.aboveAverage", { amount: formatDuration(diff) })
                : t("activity.belowAverage", { amount: formatDuration(-diff) });

    return (
        <section className="tt-card tt-act-daycard" aria-labelledby="act-day" aria-live="polite">
            <div className="tt-act-day-swap" key={day}>
                <div className="tt-card-head">
                    <div>
                        <p className="tt-eyebrow">{day === today ? t("sidebar.today") : t("activity.selectedDay")}</p>
                        <h2 id="act-day" className="tt-card-title">{longDate(day)}</h2>
                    </div>
                    <button type="button" className="tt-link tt-act-open" onClick={() => onOpenDay(day)}>
                        {t("activity.openOnToday")}
                        <ArrowUpRight size={14} aria-hidden="true" />
                    </button>
                </div>
                {total === 0 ? (
                    <p className="tt-sub">{loading ? t("limitsPage.loading") : t("activity.dayEmpty")}</p>
                ) : (
                    <>
                        <div className="tt-act-day-total">
                            <span className="tt-act-day-amount">{formatDuration(total)}</span>
                            {compare && (
                                <span className={`tt-act-compare${diff > 5 * 60 ? " tt-act-compare--up" : diff < -5 * 60 ? " tt-act-compare--down" : ""}`}>
                                    {compare}
                                </span>
                            )}
                        </div>
                        <div className="tt-act-mix" aria-hidden="true">
                            {hues.map(([h, secs]) => (
                                <span
                                    key={h}
                                    className={`tt-hue-${h}${hueFilter && hueFilter !== h ? " tt-act-seg--muted" : ""}`}
                                    style={{ flexGrow: secs }}
                                />
                            ))}
                        </div>
                        <ul className="tt-act-mix-legend">
                            {hues.map(([h, secs]) => (
                                <li key={h} className={`tt-hue-${h}`}>
                                    <span className="tt-strip-key" aria-hidden="true" />
                                    <span>{hueLabel(h)}</span>
                                    <span className="tt-act-mix-time">
                                        {formatDuration(secs)} · {Math.round((secs / total) * 100)}%
                                    </span>
                                </li>
                            ))}
                        </ul>
                        <div className="tt-act-day-apps-head">
                            <h3 className="tt-act-subtitle">{hueFilter ? t("activity.appsOf", { name: hueLabel(hueFilter) }) : t("activity.dayApps")}</h3>
                            {hueFilter && (
                                <button type="button" className="tt-link" onClick={onClearFilter}>
                                    {t("activity.showAll")}
                                </button>
                            )}
                        </div>
                        {shown.length === 0 ? (
                            <p className="tt-sub">{t("activity.noAppsInHue")}</p>
                        ) : (
                            <ol className="tt-act-day-apps">
                                {shown.map((a, i) => (
                                    <li key={a.id} className={`tt-hue-${a.hue}`} style={{ ["--i" as string]: i }}>
                                        <span className="tt-act-day-app-name">{a.label}</span>
                                        <span className="tt-act-day-app-meter" aria-hidden="true">
                                            <span style={{ width: `${(a.seconds / top) * 100}%` }} />
                                        </span>
                                        <span className="tt-act-day-app-time">{formatDuration(a.seconds)}</span>
                                    </li>
                                ))}
                            </ol>
                        )}
                    </>
                )}
            </div>
        </section>
    );
}
