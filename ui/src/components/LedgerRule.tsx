import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { formatDuration, sharePercent } from "../format";

interface LedgerRuleProps {
    summary: DaySummaryDto | null;
    loading: boolean;
    now: Date;
}

const SCALE_HOURS = ["00", "06", "12", "18", "24"] as const;

export function LedgerRule({ summary, loading, now }: LedgerRuleProps) {
    const segments = (summary?.categories ?? []).filter((category) => category.seconds > 0);
    const total = summary?.total_seconds ?? 0;
    const fraction = Math.min(
        1,
        Math.max(0, (now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()) / 86400),
    );
    const description =
        segments.length > 0
            ? `Day ledger: ${segments
                  .map((c) => `${c.label} ${formatDuration(c.seconds)} (${sharePercent(c.seconds, total)}%)`)
                  .join(", ")}`
            : "Day ledger: no time recorded yet.";
    const empty = loading || segments.length === 0;

    return (
        <div className="ledger">
            <div
                className={empty ? "ledger-band ledger-band--empty" : "ledger-band"}
                role={empty ? undefined : "img"}
                aria-label={empty ? undefined : description}
            >
                {!loading &&
                    segments.map((segment) => (
                        <span
                            key={segment.id}
                            className="ledger-seg"
                            style={{ flexGrow: segment.seconds, backgroundColor: segment.color ?? "#8d8578" }}
                        />
                    ))}
                <span className="ledger-now" style={{ left: `${fraction * 100}%` }} aria-hidden="true" />
            </div>
            <div className="ledger-scale" aria-hidden="true">
                {SCALE_HOURS.map((hour) => (
                    <span key={hour} className="ledger-scale-step">
                        <span className="ledger-scale-mark" />
                        <span className="ledger-scale-label">{hour}</span>
                    </span>
                ))}
            </div>
        </div>
    );
}
