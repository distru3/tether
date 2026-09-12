
import sys

with open("ui/src/App.tsx", "r", encoding="utf-8") as f:
    app_tsx = f.read()

app_tsx = app_tsx.replace("<LedgerRule summary={summary} loading={loading} now={now} />", "<LedgerRule summary={summary} loading={loading} now={now} catalog={catalog} />")

with open("ui/src/App.tsx", "w", encoding="utf-8") as f:
    f.write(app_tsx)


with open("ui/src/components/LedgerRule.tsx", "r", encoding="utf-8") as f:
    ledger_tsx = f.read()

new_ledger_tsx = """import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { formatDuration } from "../format";
import { colorForCategory } from "../categoryColors";

interface LedgerRuleProps {
    summary: DaySummaryDto | null;
    loading: boolean;
    now: Date;
    catalog?: CatalogDto | null;
}

const SCALE_HOURS = ["00", "06", "12", "18", "24"] as const;

export function LedgerRule({ summary, loading, now, catalog }: LedgerRuleProps) {
    const apps = summary?.apps ?? [];
    const intervals = summary?.intervals ?? [];
    const categories = summary?.categories ?? [];
    
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
                    
                    let catName = "Uncategorized";
                    if (catalog && app) {
                        const catApp = catalog.apps.find(a => a.id === app.id);
                        if (catApp) {
                            const cat = catalog.categories.find(c => c.id === catApp.primary_category);
                            if (cat) catName = cat.name;
                        }
                    }
                    
                    const color = isBlocked ? "var(--color-danger)" : colorForCategory(catName, null);
                    
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
            {!empty && categories.length > 0 && (
                <div className="ledger-legend" style={{ display: "flex", flexWrap: "wrap", gap: "16px", marginTop: "24px", justifyContent: "center" }}>
                    {categories.slice(0, 6).map(cat => (
                        <div key={cat.id} style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                            <span style={{ width: "8px", height: "8px", borderRadius: "50%", backgroundColor: colorForCategory(cat.label, cat.color) }} />
                            <span style={{ fontSize: "12px", color: "var(--text-secondary)", fontWeight: 500 }}>{cat.label}</span>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
"""

with open("ui/src/components/LedgerRule.tsx", "w", encoding="utf-8") as f:
    f.write(new_ledger_tsx)

