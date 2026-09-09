import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { formatDuration } from "../format";

interface LedgerRuleProps {
    summary: DaySummaryDto | null;
    loading: boolean;
    now: Date;
}

const SCALE_HOURS = ["00", "06", "12", "18", "24"] as const;

export function LedgerRule({ summary, loading, now }: LedgerRuleProps) {
    const apps = summary?.apps ?? [];
    const intervals = summary?.intervals ?? [];
    
    const nowSeconds = now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds();
    const fraction = Math.min(1, Math.max(0, nowSeconds / 86400));
    
    const activeSeconds = apps.filter(a => !a.blocked).reduce((acc, a) => acc + a.seconds, 0);
    const blockedSeconds = apps.filter(a => a.blocked).reduce((acc, a) => acc + a.seconds, 0);
    const idleSeconds = Math.max(0, nowSeconds - (activeSeconds + blockedSeconds));
    
    const empty = loading || (apps.length === 0 && intervals.length === 0);
    const description = `App Usage: ${formatDuration(activeSeconds)}, Limited Apps: ${formatDuration(blockedSeconds)}, Idle: ${formatDuration(idleSeconds)}`;

    return (
        <div className="ledger">
            <div
                className="ledger-band"
                role={empty ? undefined : "img"}
                aria-label={empty ? undefined : description}
                style={{ backgroundColor: "var(--bg-recessed)", position: "relative" }}
            >
                {!loading && intervals.map((interval, i) => {
                    const startDate = new Date(interval.startUtc);
                    const startSeconds = startDate.getHours() * 3600 + startDate.getMinutes() * 60 + startDate.getSeconds();
                    
                    const leftPercent = (startSeconds / 86400) * 100;
                    const widthPercent = (interval.durationSeconds / 86400) * 100;
                    
                    const app = apps.find(a => a.id === interval.appId);
                    const isBlocked = app?.blocked ?? false;
                    const color = isBlocked ? "var(--color-danger)" : (app?.color || "var(--color-primary)");
                    
                    return (
                        <span
                            key={i}
                            className="ledger-seg"
                            title={`${app?.label ?? "Unknown"} (${formatDuration(interval.durationSeconds)})`}
                            style={{ 
                                position: "absolute",
                                left: `${leftPercent}%`,
                                width: `${widthPercent}%`,
                                height: "100%",
                                backgroundColor: color
                            }}
                        />
                    );
                })}
                <span className="ledger-future" style={{ position: "absolute", left: `${fraction * 100}%`, right: 0, top: 0, bottom: 0, backgroundColor: "var(--bg-card)" }} aria-hidden="true" />
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
