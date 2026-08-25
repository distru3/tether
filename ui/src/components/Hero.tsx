import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { formatDuration, heroParts, sharePercent } from "../format";

interface HeroProps {
    summary: DaySummaryDto | null;
    loading: boolean;
}

export function Hero({ summary, loading }: HeroProps) {
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

    return (
        <section className="hero" aria-label="Today's screen time">
            <p className="hero-figure">
                {heroParts(total).map(([value, unit], index) => (
                    <span key={`${value}${unit}`}>
                        {index > 0 ? " " : ""}
                        <span>{value}</span>
                        <span className="hero-unit">{unit}</span>
                    </span>
                ))}
            </p>
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
