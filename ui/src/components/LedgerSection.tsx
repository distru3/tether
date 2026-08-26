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

    return (
        <Section label={`${label} — ${entries.length}`}>
            <ul>
                {entries.map((entry, index) => {
                    const limit = limitFor(kind, entry.id);
                    const target: LimitTargetDto =
                        kind === "app" ? { kind: "app", id: entry.id } : { kind: "category", id: entry.id };
                    return (
                        <li key={entry.id} className={entry.blocked ? "lrow lrow--blocked" : "lrow"}>
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
                    className="chip chip--ghost"
                    disabled={busy}
                    title="Categorize this app"
                    onClick={() => {
                        const app = catalog.apps.find((a) => a.id === entry.id);
                        if (app) {
                            onCategorize(entry.id, entry.label, app.primary_category, app.tags);
                        }
                    }}
                >
                    ⚙
                </button>
            )}
                        </li>
                    );
                })}
            </ul>
        </Section>
    );
}
