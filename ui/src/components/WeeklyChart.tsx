import React, { useState } from "react";
import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";
import { chartBarLabel, dayKeyToDate, formatDayLabel, formatDuration } from "../format";

interface WeeklyChartProps {
    week: WeeklySummaryDto | null;
    viewDay: number;
    loading: boolean;
    onSelectDay?: (day: number) => void;
}

const DAY_COUNT = 7;
const MAX_BAR_HEIGHT = 110;
const WEEKDAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

export function WeeklyChart({ week, viewDay, loading, onSelectDay }: WeeklyChartProps) {
    const [hoveredDay, setHoveredDay] = useState<number | null>(null);

    if (week === null) {
        if (!loading) return null;
        return (
            <div className="weekly-chart-card glass-card skeleton-loading">
                <div className="skeleton-line skeleton-title" />
                <div className="week-bars-container">
                    {Array.from({ length: DAY_COUNT }, (_, index) => (
                        <div className="week-bar-column" key={index}>
                            <div className="skeleton-bar" />
                        </div>
                    ))}
                </div>
            </div>
        );
    }

    const days = week.days;
    const total = days.reduce((sum, day) => sum + day.total_seconds, 0);
    const maxSeconds = days.reduce((max, day) => Math.max(max, day.total_seconds), 1);
    const delta =
        week.previous_week_total > 0
            ? Math.round(((total - week.previous_week_total) / week.previous_week_total) * 100)
            : null;

    const deltaLabel =
        delta === null ? "—" : delta > 0 ? `+${delta}% vs last week` : delta < 0 ? `−${Math.abs(delta)}% vs last week` : "±0% vs last week";
    const deltaClass =
        delta === null
            ? "delta-badge delta-neutral"
            : delta > 0
                ? "delta-badge delta-worse"
                : delta < 0
                    ? "delta-badge delta-better"
                    : "delta-badge delta-neutral";

    return (
        <section className="weekly-chart-card glass-card" aria-label="7-Day Activity Chart">
            <header className="chart-header">
                <div className="chart-title-block">
                    <h3 className="chart-heading">7-Day Activity</h3>
                    <div className="chart-metrics-row">
                        <span className="chart-total-time font-mono">{formatDuration(total)}</span>
                        <span className={deltaClass}>{deltaLabel}</span>
                    </div>
                </div>
            </header>

            <div className="week-bars-container" role="region" aria-label="7 Day Activity Chart">
                {days.map((day) => {
                    const isSelected = day.day === viewDay;
                    const isHovered = day.day === hoveredDay;
                    const zero = day.total_seconds === 0;
                    const heightPercent = zero ? 4 : Math.max(8, Math.round((day.total_seconds / maxSeconds) * 100));
                    const dayDate = dayKeyToDate(day.day);
                    const weekdayName = WEEKDAY_NAMES[dayDate.getDay()];
                    const fullDateStr = formatDayLabel(day.day);

                    return (
                        <div
                            className={`week-bar-column ${isSelected ? "week-bar-column--active" : ""} ${onSelectDay ? "week-bar-column--clickable" : ""}`}
                            key={day.day}
                            onClick={() => onSelectDay?.(day.day)}
                            onMouseEnter={() => setHoveredDay(day.day)}
                            onMouseLeave={() => setHoveredDay(null)}
                            onFocus={() => setHoveredDay(day.day)}
                            onBlur={() => setHoveredDay(null)}
                            tabIndex={onSelectDay ? 0 : undefined}
                            role={onSelectDay ? "button" : undefined}
                            aria-label={`${weekdayName}, ${chartBarLabel(day.day)}: ${formatDuration(day.total_seconds)}`}
                            onKeyDown={(e) => {
                                if (onSelectDay && (e.key === "Enter" || e.key === " ")) {
                                    e.preventDefault();
                                    onSelectDay(day.day);
                                }
                            }}
                        >
                            {/* Hover / Focus Tooltip */}
                            <div className={`chart-tooltip ${isHovered ? "chart-tooltip--visible" : ""}`}>
                                <span className="chart-tooltip-date">{fullDateStr}</span>
                                <span className="chart-tooltip-time font-mono">{formatDuration(day.total_seconds)}</span>
                            </div>

                            <div className="bar-track" style={{ height: `${MAX_BAR_HEIGHT}px` }}>
                                <div
                                    className={`bar-fill ${isSelected ? "bar-fill--selected" : ""}`}
                                    style={{ height: `${heightPercent}%` }}
                                >
                                    <div className="bar-glow" />
                                </div>
                            </div>
                            <span className="bar-label font-mono">{chartBarLabel(day.day)}</span>
                            <span className="bar-sublabel font-sans">{weekdayName}</span>
                        </div>
                    );
                })}
            </div>
        </section>
    );
}
