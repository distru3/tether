import { useTranslation } from "react-i18next";
import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";
import { dayKeyToDate, formatDayLabel, formatDuration, weekdayShortNames } from "../format";

interface WeeklyChartProps {
    week: WeeklySummaryDto | null;
    viewDay: number;
    loading: boolean;
    onSelectDay?: (day: number) => void;
}

/**
 * The last seven days as bars; the day being viewed is orange. Each bar is a
 * button that opens that day.
 */
export function WeeklyChart({ week, viewDay, loading, onSelectDay }: WeeklyChartProps) {
    const { t } = useTranslation();
    const names = weekdayShortNames(); // Monday first

    if (week === null) {
        return (
            <section className="tt-card tt-week" aria-labelledby="week-title" aria-busy={loading}>
                <h2 id="week-title" className="tt-card-title">{t("weeklyChart.historyTitle")}</h2>
                <div className="tt-week-bars" aria-hidden="true">
                    {Array.from({ length: 7 }, (_, i) => (
                        <span key={i} className="tt-week-col">
                            <span className="tt-week-track skeleton-shimmer" />
                        </span>
                    ))}
                </div>
            </section>
        );
    }

    const days = week.days;
    const total = days.reduce((sum, day) => sum + day.total_seconds, 0);
    const maxSeconds = days.reduce((max, day) => Math.max(max, day.total_seconds), 1);
    // A change is only meaningful when both weeks have use; a fresh install
    // would otherwise show "−100%".
    const delta =
        week.previous_week_total > 0 && total > 0 ? Math.round(((total - week.previous_week_total) / week.previous_week_total) * 100) : null;
    const vsLastWeek = t("weeklyChart.vsLastWeek");
    const deltaLabel =
        delta === null
            ? null
            : delta > 0
              ? `+${delta}% ${vsLastWeek}`
              : delta < 0
                ? `−${Math.abs(delta)}% ${vsLastWeek}`
                : `±0% ${vsLastWeek}`;

    return (
        <section className="tt-card tt-week" aria-labelledby="week-title">
            <div className="tt-card-head">
                <h2 id="week-title" className="tt-card-title">{t("weeklyChart.historyTitle")}</h2>
                <span className="tt-week-total">
                    {formatDuration(total)}
                    {deltaLabel && (
                        <span className={`tt-week-delta ${delta !== null && delta > 0 ? "tt-week-delta--up" : delta !== null && delta < 0 ? "tt-week-delta--down" : ""}`}>
                            {deltaLabel}
                        </span>
                    )}
                </span>
            </div>
            {total === 0 ? (
                <p className="tt-sub tt-week-empty">{t("weeklyChart.empty")}</p>
            ) : (
                <div className="tt-week-bars">
                    {days.map((day) => {
                        const selected = day.day === viewDay;
                        const height = day.total_seconds === 0 ? 3 : Math.max(8, Math.round((day.total_seconds / maxSeconds) * 100));
                        const date = dayKeyToDate(day.day);
                        const weekday = names[(date.getDay() + 6) % 7] ?? "";
                        const label = `${formatDayLabel(day.day)}: ${formatDuration(day.total_seconds)}`;
                        return (
                            <button
                                type="button"
                                key={day.day}
                                className={`tt-week-col${selected ? " tt-week-col--selected" : ""}`}
                                onClick={() => onSelectDay?.(day.day)}
                                aria-label={label}
                                aria-pressed={selected}
                                title={label}
                            >
                                <span className="tt-week-amount" aria-hidden="true">
                                    {selected ? formatDuration(day.total_seconds) : ""}
                                </span>
                                <span className="tt-week-track" aria-hidden="true">
                                    <span style={{ height: `${height}%` }} />
                                </span>
                                <span className="tt-week-day" aria-hidden="true">
                                    {weekday} {date.getDate()}
                                </span>
                            </button>
                        );
                    })}
                </div>
            )}
        </section>
    );
}
