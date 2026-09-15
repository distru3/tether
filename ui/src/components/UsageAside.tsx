import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { formatDuration } from "../format";
import { colorForCategory } from "../categoryColors";
import { LiveTimer } from "./LiveTimer";

interface UsageAsideProps {
  entries: UsageRowDto[];
  total: number;
  catalog?: CatalogDto | null;
  onCategorize?: (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => void;
  onOpenAppDirectory?: () => void;
}

export function UsageAside({ entries, total, catalog, onCategorize, onOpenAppDirectory }: UsageAsideProps) {
  const ranked = [...entries].sort((a, b) => b.seconds - a.seconds).slice(0, 5);
  const max = Math.max(ranked[0]?.seconds ?? 0, 1);

  return (
    <aside className="usage-aside dashboard-panel" aria-label="Most used applications">
      <div className="panel-heading-row">
        <div>
          <p className="panel-eyebrow">Your activity</p>
          <h3>Most used</h3>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {onOpenAppDirectory && (
            <button
              type="button"
              className="usage-manage-apps-btn"
              onClick={onOpenAppDirectory}
              title="Browse and categorize all applications"
            >
              All apps &rarr;
            </button>
          )}
          <span className="panel-count">{ranked.length}</span>
        </div>
      </div>
      {ranked.length === 0 ? (
        <div className="usage-empty">No application activity recorded yet.</div>
      ) : (
        <div className="usage-list">
          {ranked.map((entry, index) => {
            const percent = total > 0 ? Math.round((entry.seconds / total) * 100) : 0;
            const width = Math.max(8, Math.round((entry.seconds / max) * 100));
            const appObj = catalog?.apps.find((a) => a.id === entry.id);
            const category = appObj ? catalog?.categories.find((c) => c.id === appObj.primary_category) : null;
            const catName = category?.name ?? "Uncategorized";
            const catColor = colorForCategory(catName, category?.color ?? entry.color, index);

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
                    <span className="usage-bar-fill" style={{ width: `${width}%`, backgroundColor: catColor }} />
                  </div>
                  <div className="usage-app-subline">
                    <small>{percent}% of today</small>
                    {onCategorize ? (
                      <button
                        type="button"
                        className="usage-category-pill usage-category-pill--clickable"
                        style={{
                          color: catColor,
                          backgroundColor: `${catColor}1c`,
                          borderColor: `${catColor}38`,
                        }}
                        onClick={() => onCategorize(entry.id, entry.label, appObj?.primary_category ?? null, appObj?.tags ?? [])}
                        title="Click to change category"
                      >
                        {catName}
                      </button>
                    ) : (
                      <span 
                        className="usage-category-pill"
                        style={{
                          color: catColor,
                          backgroundColor: `${catColor}1c`,
                          borderColor: `${catColor}38`,
                        }}
                      >
                        {catName}
                      </span>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </aside>
  );
}
