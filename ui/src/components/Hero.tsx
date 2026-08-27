import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { formatDayLabel, formatDuration, heroParts, sharePercent } from "../format";

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
    if (loading || summary === null) {
        return (
            <section className="hero" aria-label="Today's screen time">
                {loading ? (
                    <div aria-hidden="true">
                        <span className="skel skel-figure" />
                        <span className="skel skel-line" />
                    </div>
                ) : (
                    <p className="hero-sub">The ledger is unreachable right now.</p>
                )}
            </section>
        );
    }

    const total = summary.total_seconds;
    const lead = summary.categories.find((category) => category.seconds > 0);
    const canBrowse = viewDay !== undefined && goPrevDay !== undefined && goNextDay !== undefined;

    return (
        <section className="hero" aria-label={isViewingToday ? "Today's screen time" : "Screen time"}>
            {canBrowse && !isViewingToday && (
                <div className="hero-toolbar">
                    <p className="hero-datelabel">{formatDayLabel(viewDay)}</p>
                    {goToday !== undefined && (
                        <button type="button" className="chip" onClick={goToday}>
                            Today
                        </button>
                    )}
                </div>
            )}
            <div className="hero-row">
                {canBrowse && (
                    <button type="button" className="daynav" aria-label="Previous day" onClick={goPrevDay}>
                        ‹
                    </button>
                )}
                <p className="hero-figure">
                    {heroParts(total).map(([value, unit], index) => (
                        <span key={`${value}${unit}`}>
                            {index > 0 ? " " : ""}
                            <span>{value}</span>
                            <span className="hero-unit">{unit}</span>
                        </span>
                    ))}
                </p>
                {canBrowse && (
                    <button
                        type="button"
                        className="daynav"
                        aria-label="Next day"
                        onClick={goNextDay}
                        disabled={isViewingToday}
                    >
                        ›
                    </button>
                )}
            </div>
            {lead ? (
                <p className="hero-sub">
                    Led by {lead.label} · {formatDuration(lead.seconds)} · {sharePercent(lead.seconds, total)}%
                </p>
            ) : (
                <p className="hero-sub">A clean slate. Go make something.</p>
            )}
        </section>
    );
}
