import { useTranslation } from "react-i18next";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { formatDuration } from "../format";
import { colorForCategory } from "../categoryColors";

interface CategoryMixProps {
  categories: UsageRowDto[];
  total: number;
}

export function CategoryMix({ categories, total }: CategoryMixProps) {
  const { t } = useTranslation();
  const rows = [...categories].sort((a, b) => b.seconds - a.seconds).slice(0, 4);
  let cursor = 0;
  const stops = rows.map((row, index) => {
    const start = cursor;
    const share = total > 0 ? (row.seconds / total) * 100 : 0;
    cursor += share;
    const color = colorForCategory(row.label, row.color, index);
    return `${color} ${start}% ${cursor}%`;
  });
  if (cursor < 100) stops.push(`var(--border-strong, rgba(128,128,128,0.2)) ${cursor}% 100%`);

  return (
    <section className="category-mix dashboard-panel" aria-label={t("mix.title")}>
      <div className="panel-heading-row">
        <div>
          <p className="panel-eyebrow">{t("mix.eyebrow")}</p>
          <h3>{t("mix.title")}</h3>
        </div>
      </div>
      <div className="mix-content">
        <div className="mix-donut" style={{ background: `conic-gradient(${stops.join(", ")})` }}>
          <div className="mix-donut-hole">
            <strong>{formatDuration(total)}</strong>
            <span>{t("mix.tracked")}</span>
          </div>
        </div>
        <div className="mix-legend">
          {rows.length === 0 ? (
            <span className="usage-empty">{t("mix.empty")}</span>
          ) : rows.map((row, index) => {
            const color = colorForCategory(row.label, row.color, index);
            const percent = total > 0 ? Math.round((row.seconds / total) * 100) : 0;
            return (
              <div className="mix-legend-row" key={row.id}>
                <span className="mix-dot" style={{ background: color }} />
                <span>{row.label}</span>
                <strong>{percent}%</strong>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
