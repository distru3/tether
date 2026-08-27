import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";
import { chartBarLabel, dayKeyToDate, formatDuration } from "../format";
import { Section } from "./Section";

interface WeeklyChartProps {
    week: WeeklySummaryDto | null;
    viewDay: number;
    loading: boolean;
}

const DAY_COUNT = 7;
const MAX_BAR_PX = 96;

const WEEKDAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

/** Oldest bar sits at 38% ink, scaling to full ink on the newest. */
function barOpacity(index: number): number {
    return 0.38 + (0.62 * index) / (DAY_COUNT - 1);
}

export function WeeklyChart({ week, viewDay, loading }: WeeklyChartProps) {
    if (week === null) {
        if (!loading) return null;
        return (
            <Section label="Past 7 days">
                <div className="week" aria-hidden="true">
                    {Array.from({ length: DAY_COUNT }, (_, index) => (
                        <div className="week-col" key={index}>
                            <div className="week-track">
                                <span className="skel skel-bar" />
                            </div>
                            <span className="skel skel-label" />
                        </div>
                    ))}
                </div>
            </Section>
        );
    }

    const days = week.days;
    const total = days.reduce((sum, day) => sum + day.total_seconds, 0);
    const maxSeconds = days.reduce((max, day) => Math.max(max, day.total_seconds), 0);
    const delta =
        week.previous_week_total > 0
            ? Math.round(((total - week.previous_week_total) / week.previous_week_total) * 100)
            : null;
    const deltaLabel = delta === null ? "—" : delta > 0 ? `+${delta}%` : delta < 0 ? `−${Math.abs(delta)}%` : "±0%";
    const deltaClass =
        delta === null
            ? "week-comp week-comp--na"
            : delta > 0
              ? "week-comp week-comp--worse"
              : delta < 0
                ? "week-comp week-comp--better"
                : "week-comp week-comp--flat";

    const aria = `${days
        .map((day) => `${WEEKDAY_NAMES[dayKeyToDate(day.day).getDay()]} ${formatDuration(day.total_seconds)}`)
        .join(", ")}.`;

    return (
        <Section label={`PAST 7 DAYS — ${formatDuration(total)}`}>
            <p className={deltaClass}>last week: {formatDuration(week.previous_week_total)} · ({deltaLabel})</p>
            <div className="week" role="img" aria-label={`Bar chart of the last seven days: ${aria}`}>
                {days.map((day, index) => {
                    const selected = day.day === viewDay;
                    const zero = day.total_seconds === 0;
                    const px = zero ? 0 : Math.max(2, Math.round((day.total_seconds / maxSeconds) * MAX_BAR_PX));
                    return (
                        <div className="week-col" key={day.day}>
                            {selected && <span className="week-tick" aria-hidden="true" />}
                            <div className="week-track">
                                {!zero && (
                                    <span
                                        className={selected ? "week-bar week-bar--selected" : "week-bar"}
                                        style={{ height: `${px}px`, opacity: selected ? 1 : barOpacity(index) }}
                                    />
                                )}
                            </div>
                            <span className={zero ? "week-label week-label--zero" : "week-label"}>{chartBarLabel(day.day)}</span>
                        </div>
                    );
                })}
            </div>
        </Section>
    );
}
