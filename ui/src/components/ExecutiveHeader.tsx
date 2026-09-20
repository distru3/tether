import React from "react";
import { useTranslation } from "react-i18next";
import { Clock, Flame, Hourglass, ShieldCheck, ChevronLeft, ChevronRight } from "lucide-react";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { LimitDto } from "../types/generated/LimitDto";
import { formatDayLabel, formatDuration, heroParts, sharePercent } from "../format";
import "./ExecutiveHeader.css";

interface ExecutiveHeaderProps {
  summary: DaySummaryDto | null;
  loading: boolean;
  total: number;
  apps: UsageRowDto[];
  viewDay?: number;
  isViewingToday: boolean;
  goPrevDay: () => void;
  goNextDay: () => void;
  goToday?: () => void;
  maxIntervalSeconds: number;
  maxAppName: string | null;
  streakBadge?: string;
  streakSubtitle: string;
  enabledLimits: LimitDto[];
  remainingBudgetSeconds: number;
  blockedCount: number;
  blocked: UsageRowDto[];
  vsAvgText: string;
}

export function ExecutiveHeader({
  summary,
  loading,
  total,
  apps,
  viewDay,
  isViewingToday,
  goPrevDay,
  goNextDay,
  goToday,
  maxIntervalSeconds,
  streakBadge,
  streakSubtitle,
  enabledLimits,
  remainingBudgetSeconds,
  blockedCount,
  blocked,
  vsAvgText,
}: ExecutiveHeaderProps) {
  const { t } = useTranslation();

  const lead = summary?.categories?.find((category) => category.seconds > 0);
  const parts = heroParts(total);

  return (
    <section className="executive-header" aria-label="Screen time telemetry and summary">
      {/* Primary Hero Zone */}
      <div className="executive-hero">
        <div className="executive-hero__top">
          <div className="executive-date-stepper">
            <button
              type="button"
              className="exec-nav-btn"
              onClick={goPrevDay}
              aria-label={t("hero.prevDay", "Previous Day")}
              title={t("hero.prevDay", "Previous Day")}
            >
              <ChevronLeft size={14} />
            </button>
            <span className="exec-date-label">
              {isViewingToday ? t("hero.today", "Today") : viewDay ? formatDayLabel(viewDay) : ""}
            </span>
            <button
              type="button"
              className="exec-nav-btn"
              onClick={goNextDay}
              disabled={isViewingToday}
              aria-label={t("hero.nextDay", "Next Day")}
              title={t("hero.nextDay", "Next Day")}
            >
              <ChevronRight size={14} />
            </button>
          </div>

          {!isViewingToday && goToday && (
            <button type="button" className="exec-today-pill" onClick={goToday}>
              {t("hero.backToToday", "Jump to Today")}
            </button>
          )}

          {lead && total > 0 && (
            <div className="exec-lead-chip">
              <span
                className="exec-lead-dot"
                style={{ backgroundColor: lead.color || "var(--color-primary)" }}
              />
              <span className="exec-lead-text">
                {lead.label} · {sharePercent(lead.seconds, total)}
              </span>
            </div>
          )}
        </div>

        <div className="executive-time-readout">
          {parts.map(([val, unit]) => (
            <span key={`${val}${unit}`} className="exec-time-cluster">
              <span className="exec-time-val">{val}</span>
              <span className="exec-time-unit">{unit}</span>
            </span>
          ))}
          {vsAvgText && (
            <span className="exec-trend-caption">{vsAvgText}</span>
          )}
        </div>
      </div>

      {/* Telemetry Indicator Zone */}
      <div className="executive-telemetry">
        {/* Metric 1: Focus Streak */}
        <div className="exec-metric-cell">
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              <Flame size={14} color="var(--color-warning)" />
            </span>
            <span className="exec-metric-kicker">LONGEST STREAK</span>
            {streakBadge && (
              <span className="exec-metric-badge exec-metric-badge--streak">{streakBadge}</span>
            )}
          </div>
          <div className="exec-metric-value">
            {maxIntervalSeconds > 0 ? formatDuration(maxIntervalSeconds) : "—"}
          </div>
          <div className="exec-metric-sub">{streakSubtitle}</div>
        </div>

        {/* Metric 2: Remaining Budget */}
        <div className="exec-metric-cell">
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              <Hourglass size={14} color="var(--color-primary)" />
            </span>
            <span className="exec-metric-kicker">DAILY ALLOWANCE</span>
            {enabledLimits.length > 0 && (
              <span
                className={`exec-metric-badge ${
                  remainingBudgetSeconds === 0
                    ? "exec-metric-badge--exhausted"
                    : "exec-metric-badge--budget"
                }`}
              >
                {remainingBudgetSeconds === 0 ? "Exhausted" : "Active"}
              </span>
            )}
          </div>
          <div className="exec-metric-value">
            {enabledLimits.length > 0 ? formatDuration(remainingBudgetSeconds) : "No limits"}
          </div>
          <div className="exec-metric-sub">
            {enabledLimits.length > 0
              ? `${enabledLimits.length} active limit${enabledLimits.length === 1 ? "" : "s"}`
              : "Unrestricted today"}
          </div>
        </div>

        {/* Metric 3: Protection Status */}
        <div className="exec-metric-cell">
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              <ShieldCheck
                size={14}
                color={blockedCount > 0 ? "var(--color-danger)" : "var(--color-success)"}
              />
            </span>
            <span className="exec-metric-kicker">PROTECTION</span>
            <span
              className={`exec-metric-badge ${
                blockedCount > 0
                  ? "exec-metric-badge--blocked"
                  : "exec-metric-badge--protected"
              }`}
            >
              {blockedCount > 0 ? `${blockedCount} Blocked` : "Active"}
            </span>
          </div>
          <div className="exec-metric-value">
            {blockedCount > 0 ? `${blockedCount} Suspended` : "Enforced"}
          </div>
          <div className="exec-metric-sub">
            {blockedCount > 0
              ? `${blocked.map((a) => a.label).join(", ")} suspended`
              : "Zero limit violations"}
          </div>
        </div>
      </div>
    </section>
  );
}
