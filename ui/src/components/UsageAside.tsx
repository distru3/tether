import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { useTranslation } from "react-i18next";
import { formatDuration } from "../format";
import { hueForApp } from "../todayModel";
import { LiveTimer } from "./LiveTimer";

interface UsageAsideProps {
  entries: UsageRowDto[];
  total: number;
  catalog?: CatalogDto | null;
  onCategorize?: (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => void;
  onOpenAppDirectory?: () => void;
}

/**
 * The day's five most used apps. Bars take the colour of the budget the app
 * counts toward (as on the day strip); the category is a neutral button that
 * opens the categorize dialog.
 */
export function UsageAside({ entries, total, catalog = null, onCategorize, onOpenAppDirectory }: UsageAsideProps) {
  const { t } = useTranslation();
  const ranked = [...entries].sort((a, b) => b.seconds - a.seconds).slice(0, 5);
  const max = Math.max(ranked[0]?.seconds ?? 0, 1);

  return (
    <section className="tt-card tt-usage" aria-labelledby="usage-title">
      <div className="tt-card-head">
        <h2 id="usage-title" className="tt-card-title">{t("usage.mostUsed")}</h2>
        {onOpenAppDirectory && (
          <button type="button" className="tt-link" onClick={onOpenAppDirectory} title={t("usage.allAppsHint")}>
            {t("usage.allApps")}
          </button>
        )}
      </div>
      {ranked.length === 0 ? (
        <p className="tt-sub">{t("usage.empty")}</p>
      ) : (
        <ol className="tt-usage-list">
          {ranked.map((entry) => {
            const percent = total > 0 ? Math.round((entry.seconds / total) * 100) : 0;
            const width = Math.max(4, Math.round((entry.seconds / max) * 100));
            const appObj = catalog?.apps.find((a) => a.id === entry.id);
            const category = appObj ? catalog?.categories.find((c) => c.id === appObj.primary_category) : null;
            const catName = category?.name ?? t("categorize.uncategorized");
            const { hue } = hueForApp(entry.id, catalog);
            return (
              <li key={entry.id} className={`tt-usage-row tt-hue-${hue}`}>
                <div className="tt-usage-line">
                  <span className="tt-usage-name">
                    <strong>{entry.label}</strong>
                    {entry.timer_expires_utc && <LiveTimer expiresUtc={entry.timer_expires_utc} showLabel label="+15m" />}
                  </span>
                  <span className="tt-usage-time">{formatDuration(entry.seconds)}</span>
                </div>
                <div className="tt-usage-bar" aria-hidden="true">
                  <span style={{ width: `${width}%` }} />
                </div>
                <div className="tt-usage-sub">
                  <span>{t("usage.shareOfDay", { percent })}</span>
                  {onCategorize ? (
                    <button
                      type="button"
                      className="tt-usage-cat"
                      onClick={() => onCategorize(entry.id, entry.label, appObj?.primary_category ?? null, appObj?.tags ?? [])}
                      title={t("usage.changeCategory")}
                    >
                      {catName}
                    </button>
                  ) : (
                    <span className="tt-usage-cat">{catName}</span>
                  )}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
