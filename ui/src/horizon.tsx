import type { DaySummary, UsageRow } from "./types";

interface HorizonProps {
    summary: DaySummary;
    nowMarker?: boolean;
}

/** The signature: the whole day as one frontier line.
 *
 * Each category occupies its proportional share in its own colour. A bright
 * marker sits at the current fraction of the day elapsed, so "spent" visibly
 * approaches it. The spent portion is shaded, the remaining budget is dim. */
export function Horizon({ summary, nowMarker = true }: HorizonProps) {
    const total = summary.totalSeconds;
    const cats = summary.categories;
    const now = new Date();
    const dayFraction = (now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()) / 86400;
    const markerPct = Math.min(100, dayFraction * 100);

    return (
        <div className="horizon" aria-hidden="true">
            <div className="horizon-track">
                {cats.length > 0
                    ? cats.map((c) => (
                          <span
                              key={c.id}
                              className="horizon-seg"
                              style={{
                                  background: c.color ?? "#94a3b8",
                                  flexGrow: c.seconds,
                              }}
                          />
                      ))
                    : <span className="horizon-seg horizon-empty" />}
            </div>
            {nowMarker && (
                <span className="horizon-marker" style={{ left: `${markerPct}%` }} title="now" />
            )}
            <div className="horizon-scale" aria-hidden="true">
                <span>00:00</span>
                <span>12:00</span>
                <span>now</span>
            </div>
        </div>
    );
}

/** A compact category chip legend under the hero. */
export function Legend({ categories }: { categories: UsageRow[] }) {
    return (
        <ul className="legend">
            {categories.map((c) => (
                <li key={c.id} className="legend-item">
                    <span className="legend-swatch" style={{ background: c.color ?? "#94a3b8" }} />
                    <span className="legend-name">{c.label}</span>
                </li>
            ))}
        </ul>
    );
}