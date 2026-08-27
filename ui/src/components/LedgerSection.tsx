import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { formatDuration, sharePercent } from "../format";
import { Section } from "./Section";

type RowKind = "app" | "category";

interface LedgerSectionProps {
    label: string;
    kind: RowKind;
    entries: UsageRowDto[];
    total: number;
    limitFor: (kind: RowKind, id: number) => LimitDto | undefined;
    canLimit: (entry: UsageRowDto) => boolean;
    busy: boolean;
    onEdit: (target: LimitTargetDto, limit: LimitDto | null) => void;
    onCategorize?: (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => void;
    catalog?: CatalogDto | null;
}

function primaryCategoryName(catalog: CatalogDto, appId: number): string | null {
    const app = catalog.apps.find((a) => a.id === appId);
    if (!app) return null;
    return catalog.categories.find((c) => c.id === app.primary_category)?.name ?? null;
}

function isUncategorizedApp(catalog: CatalogDto, appId: number): boolean {
    const app = catalog.apps.find((a) => a.id === appId);
    if (!app) return false;
    const category = catalog.categories.find((c) => c.id === app.primary_category);
    return category !== undefined && (category.slug === "uncategorized" || category.name === "Uncategorized");
}

function limitPercent(seconds: number, limitSeconds: number): number {
    if (limitSeconds <= 0) return 100;
    return Math.min(100, (seconds / limitSeconds) * 100);
}

export function LedgerSection({
    label,
    kind,
    entries,
    total,
    limitFor,
    canLimit,
    busy,
    onEdit,
    onCategorize,
    catalog,
}: LedgerSectionProps) {
    if (entries.length === 0) return null;

    const uncategorized: UsageRowDto[] = [];
    if (kind === "app" && onCategorize && catalog) {
        for (const entry of entries) {
            if (isUncategorizedApp(catalog, entry.id)) uncategorized.push(entry);
        }
    }

    return (
        <Section label={`${label} — ${entries.length}`}>
            <ul>
                {entries.map((entry, index) => {
                    const limit = limitFor(kind, entry.id);
                    const target: LimitTargetDto =
                        kind === "app" ? { kind: "app", id: entry.id } : { kind: "category", id: entry.id };
                    const limitSeconds = entry.limit_seconds;
                    const overLimit = limitSeconds !== null && entry.seconds >= limitSeconds;
                    return (
                        <li key={entry.id} className={entry.blocked ? "lrow lrow--blocked" : "lrow"}>
                            <div className="lrow-line">
                                <span className="lrow-rank" aria-hidden="true">
                                    {index + 1}
                                </span>
                                <span
                                    className="swatch"
                                    style={{ backgroundColor: entry.color ?? "#8d8578" }}
                                    aria-hidden="true"
                                />
                                <span className="lrow-name">
                                    {entry.label}
                                    {entry.blocked && <span className="blocked-tag">Blocked</span>}
                                </span>
                                <span className="lrow-leader" aria-hidden="true" />
                                <span className="lrow-time">{formatDuration(entry.seconds)}</span>
                                <span className="lrow-share">{sharePercent(entry.seconds, total)}%</span>
                                {limit ? (
                                    <button
                                        type="button"
                                        className="chip"
                                        disabled={busy}
                                        title={`Edit the ${limit.default_minutes}m/day order`}
                                        onClick={() => onEdit(limit.target, limit)}
                                    >
                                        {limit.default_minutes}m/day
                                    </button>
                                ) : canLimit(entry) ? (
                                    <button
                                        type="button"
                                        className="chip chip--ghost"
                                        disabled={busy}
                                        title="Set a daily order"
                                        onClick={() => onEdit(target, null)}
                                    >
                                        + set
                                    </button>
                                ) : null}
                                {kind === "app" && onCategorize && catalog && (
                                    <button
                                        type="button"
                                        className={
                                            isUncategorizedApp(catalog, entry.id) ? "catchip catchip--uncat" : "catchip"
                                        }
                                        disabled={busy}
                                        title="Change category"
                                        onClick={() => {
                                            const app = catalog.apps.find((a) => a.id === entry.id);
                                            if (app) {
                                                onCategorize(entry.id, entry.label, app.primary_category, app.tags);
                                            }
                                        }}
                                    >
                                        {primaryCategoryName(catalog, entry.id) ?? "—"}
                                    </button>
                                )}
                            </div>
                            {limitSeconds !== null && (
                                <div className="lrow-bar" aria-hidden="true">
                                    <div
                                        className={overLimit ? "lrow-bar-fill lrow-bar-fill--over" : "lrow-bar-fill"}
                                        style={{ width: `${limitPercent(entry.seconds, limitSeconds)}%` }}
                                    />
                                </div>
                            )}
                        </li>
                    );
                })}
            </ul>
            {uncategorized.length > 0 && onCategorize && catalog && (
                <p className="panel-note">
                    <span className="note-faint">
                        {uncategorized.length} app{uncategorized.length === 1 ? "" : "s"} uncategorized
                    </span>
                    {" · "}
                    <button
                        type="button"
                        className="textbtn"
                        disabled={busy}
                        onClick={() => {
                            const first = uncategorized[0];
                            const app = first === undefined ? undefined : catalog.apps.find((a) => a.id === first.id);
                            if (first !== undefined && app !== undefined) {
                                onCategorize(first.id, first.label, app.primary_category, app.tags);
                            }
                        }}
                    >
                        categorize them
                    </button>
                </p>
            )}
        </Section>
    );
}
