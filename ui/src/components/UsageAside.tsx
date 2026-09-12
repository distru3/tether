import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { formatDuration } from "../format";
import { LiveTimer } from "./LiveTimer";

interface UsageAsideProps {
  entries: UsageRowDto[];
  total: number;
}

export function UsageAside({ entries, total }: UsageAsideProps) {
  const ranked = [...entries].sort((a, b) => b.seconds - a.seconds);
  const max = Math.max(ranked[0]?.seconds ?? 0, 1);

  return (
    <aside className="usage-aside dashboard-panel" aria-label="Most used applications">
      <div className="panel-heading-row">
        <div>
          <p className="panel-eyebrow">Your activity</p>
          <h3>Most used</h3>
        </div>
        <span className="panel-count">{ranked.length}</span>
      </div>
      {ranked.length === 0 ? (
        <div className="usage-empty">No application activity recorded yet.</div>
      ) : (
        <div className="usage-list">
          {ranked.map((entry, index) => {
            const percent = total > 0 ? Math.round((entry.seconds / total) * 100) : 0;
            const width = Math.max(8, Math.round((entry.seconds / max) * 100));
            return (
              <div className="usage-list-row" key={entry.id}>
                <span className="usage-rank">0{index + 1}</span>
                <div className="usage-app-copy">
                  <div className="usage-app-line">
                    <div style={{ display: "inline-flex", alignItems: "center", gap: 6, minWidth: 0 }}>
                      <strong style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{entry.label}</strong>
                      {entry.timer_expires_utc && (
                        <LiveTimer expiresUtc={entry.timer_expires_utc} showLabel label="+15m" />
                      )}
                    </div>
                    <span>{formatDuration(entry.seconds)}</span>
                  </div>
                  <div className="usage-bar-track">
                    <span className="usage-bar-fill" style={{ width: `${width}%` }} />
                  </div>
                  <small>{percent}% of today</small>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </aside>
  );
}
