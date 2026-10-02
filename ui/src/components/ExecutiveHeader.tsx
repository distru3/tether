import { useTranslation } from "react-i18next";
import { Flame, Hourglass, ShieldCheck, ShieldOff } from "lucide-react";
import { formatDayLabel, formatDuration, heroParts, sharePercent } from "../format";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import "./ExecutiveHeader.css";

/** The limit with the least time left today, or every limit already reached. */
export interface LimitOutlook {
  /** Limits whose budget is used up, by display label. */
  reached: string[];
  /** The nearest not-yet-reached limit. */
  closest: { label: string; remainingSeconds: number } | null;
}

interface ExecutiveHeaderProps {
  total: number;
  /** Primary-category rows for the viewed day, descending. */
  categories: UsageRowDto[];
  viewDay: number;
  isViewingToday: boolean;
  goToday: () => void;
  longestStretchSeconds: number;
  longestStretchApp: string | null;
  /** Percent vs the 7-day average, or null when there is no baseline yet. */
  vsAveragePercent: number | null;
  limits: LimitOutlook;
  /** Whether any limit is configured; null while the catalog is unknown. */
  hasLimits: boolean | null;
  /** The agent answered the last poll. Protection is unknown otherwise. */
  agentReachable: boolean;
  familyDnsEnabled: boolean;
  blockedCount: number;
}

export function ExecutiveHeader({
  total,
  categories,
  viewDay,
  isViewingToday,
  goToday,
  longestStretchSeconds,
  longestStretchApp,
  vsAveragePercent,
  limits,
  hasLimits,
  agentReachable,
  familyDnsEnabled,
  blockedCount,
}: ExecutiveHeaderProps) {
  const { t } = useTranslation();
  const lead = categories.find((category) => category.seconds > 0);

  const trend =
    vsAveragePercent === null
      ? null
      : vsAveragePercent === 0
        ? t("hero.onAverage")
        : t(vsAveragePercent > 0 ? "hero.aboveAverage" : "hero.belowAverage", {
            percent: Math.abs(vsAveragePercent),
          });

  return (
    <section className="executive-header" aria-label={t("hero.summaryLabel")}>
      <div className="executive-hero">
        <div className="executive-hero__top">
          <span className="exec-date-label">
            {isViewingToday ? t("hero.today") : formatDayLabel(viewDay)}
          </span>
          {!isViewingToday && (
            <button type="button" className="exec-today-pill" onClick={goToday}>
              {t("hero.backToToday")}
            </button>
          )}
        </div>

        <div className="executive-time-readout">
          <span className="exec-time-figure" aria-label={formatDuration(total)}>
            {heroParts(total).map(([value, unit]) => (
              <span key={unit} className="exec-time-cluster" aria-hidden="true">
                <span className="exec-time-val">{value}</span>
                <span className="exec-time-unit">{unit}</span>
              </span>
            ))}
          </span>
          {trend && <span className="exec-trend-caption">{trend}</span>}
        </div>

        {lead && total > 0 && (
          <div className="exec-lead-chip">
            <span className="exec-lead-dot" style={{ backgroundColor: lead.color || "var(--color-primary)" }} />
            <span className="exec-lead-text">
              {t("hero.mostlyIn", { label: lead.label, percent: sharePercent(lead.seconds, total) })}
            </span>
          </div>
        )}
      </div>

      <div className="executive-telemetry">
        <div className="exec-metric-cell">
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              <Flame size={14} color="var(--color-warning)" aria-hidden="true" />
            </span>
            <span className="exec-metric-kicker">{t("hero.longestStretch")}</span>
          </div>
          <div className="exec-metric-value">
            {longestStretchSeconds > 0 ? formatDuration(longestStretchSeconds) : "—"}
          </div>
          <div className="exec-metric-sub">
            {longestStretchSeconds > 0 && longestStretchApp
              ? t("hero.stretchIn", { app: longestStretchApp })
              : t("hero.noStretch")}
          </div>
        </div>

        <div className="exec-metric-cell">
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              <Hourglass size={14} color="var(--color-primary)" aria-hidden="true" />
            </span>
            <span className="exec-metric-kicker">{t("hero.closestLimit")}</span>
            {limits.reached.length > 0 && (
              <span className="exec-metric-badge exec-metric-badge--exhausted">
                {t("hero.reachedCount", { count: limits.reached.length })}
              </span>
            )}
          </div>
          <div className="exec-metric-value">
            {hasLimits === null
              ? "—"
              : !hasLimits
              ? t("hero.noLimits")
              : limits.closest
                ? t("hero.timeLeft", { duration: formatDuration(limits.closest.remainingSeconds) })
                : t("hero.allReached")}
          </div>
          <div className="exec-metric-sub">
            {hasLimits === null
              ? ""
              : !hasLimits
              ? t("hero.noLimitsHint")
              : limits.closest
                ? limits.closest.label
                : limits.reached.join(", ")}
          </div>
        </div>

        <div className={`exec-metric-cell ${agentReachable ? "" : "exec-metric-cell--alert"}`}>
          <div className="exec-metric-label-row">
            <span className="exec-metric-icon">
              {agentReachable ? (
                <ShieldCheck size={14} color="var(--color-success)" aria-hidden="true" />
              ) : (
                <ShieldOff size={14} color="var(--color-danger)" aria-hidden="true" />
              )}
            </span>
            <span className="exec-metric-kicker">{t("hero.protection")}</span>
            <span
              className={`exec-metric-badge ${agentReachable ? "exec-metric-badge--protected" : "exec-metric-badge--blocked"}`}
            >
              {agentReachable ? t("hero.protectionOn") : t("hero.protectionUnknown")}
            </span>
          </div>
          <div className="exec-metric-value">
            {agentReachable ? t("hero.enforcing") : t("hero.notRunning")}
          </div>
          <div className="exec-metric-sub">
            {!agentReachable
              ? t("hero.notRunningHint")
              : [
                  blockedCount > 0 ? t("hero.blockedNow", { count: blockedCount }) : t("hero.nothingBlocked"),
                  familyDnsEnabled ? t("hero.familyDnsOn") : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
          </div>
        </div>
      </div>
    </section>
  );
}
