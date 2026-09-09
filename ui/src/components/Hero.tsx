import { useTranslation } from "react-i18next";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { formatDayLabel, formatDuration, heroParts, sharePercent } from "../format";
import { ChevronLeftIcon, ChevronRightIcon, SparklesIcon } from "./icons/Icons";

interface HeroProps {
    summary: DaySummaryDto | null;
    loading: boolean;
    viewDay?: number;
    isViewingToday?: boolean;
    goPrevDay?: () => void;
    goNextDay?: () => void;
    goToday?: () => void;
}

export function Hero({
    summary,
    loading,
    viewDay,
    isViewingToday = true,
    goPrevDay,
    goNextDay,
    goToday,
}: HeroProps) {
    const { t } = useTranslation();

    if (summary === null) {
        return (
            <div className="hero-card glass-card skeleton-loading">
                <div className="skeleton-line skeleton-title" />
                <div className="skeleton-line skeleton-subtitle" />
            </div>
        );
    }

    const total = summary.total_seconds;
    const lead = summary.categories.find((category) => category.seconds > 0);
    const canBrowse = viewDay !== undefined && goPrevDay !== undefined && goNextDay !== undefined;

    return (
        <section className="hero-card glass-card" aria-label="Daily screen time summary">
            <div className="hero-content">
                <header className="hero-meta">
                    <h2 className="hero-heading">
                        {isViewingToday ? t("hero.today") : viewDay ? formatDayLabel(viewDay) : t("hero.dayView")}
                    </h2>
                    {!isViewingToday && goToday && (
                        <button type="button" className="btn btn-secondary btn-sm" onClick={goToday}>
                            {t("hero.backToToday")}
                        </button>
                    )}
                </header>

                <div className="hero-time-cluster">
                    {canBrowse && (
                        <button type="button" className="stepper-nav-btn" aria-label={t("hero.prevDay")} onClick={goPrevDay} title={t("hero.prevDay")}>
                            <ChevronLeftIcon size={16} />
                        </button>
                    )}

                    <div className="hero-time-readout">
                        {heroParts(total).map(([value, unit]) => (
                            <span key={`${value}${unit}`} className="time-segment">
                                <span className="time-value font-mono">{value}</span>
                                <span className="time-unit">{unit}</span>
                            </span>
                        ))}
                    </div>

                    {canBrowse && (
                        <button
                            type="button"
                            className="stepper-nav-btn"
                            aria-label={t("hero.nextDay")}
                            onClick={goNextDay}
                            disabled={isViewingToday}
                            title={t("hero.nextDay")}
                        >
                            <ChevronRightIcon size={16} />
                        </button>
                    )}
                </div>

                <footer className="hero-context">
                    {lead ? (
                        <div className="lead-category-chip">
                            <span className="lead-dot" style={{ backgroundColor: lead.color || "var(--accent-indigo)" }} />
                            <span 
                                className="lead-text" 
                                dangerouslySetInnerHTML={{ 
                                    __html: t("hero.mostTimeIn", { 
                                        label: lead.label, 
                                        duration: formatDuration(lead.seconds), 
                                        percent: sharePercent(lead.seconds, total) 
                                    }) 
                                }} 
                            />
                        </div>
                    ) : (
                        <div className="clean-slate-container">
                            <SparklesIcon size={16} color="var(--accent-emerald)" />
                            <span className="clean-slate-text">{isViewingToday ? t("hero.noUsage") : t("hero.noUsageDay")}</span>
                        </div>
                    )}
                </footer>
            </div>
        </section>
    );
}
