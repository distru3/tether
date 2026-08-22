import type { Catalog, LimitInfo, LimitTarget, UsageRow } from "./types";
import { formatDuration, percent } from "./types";

interface UsageRowProps {
    row: UsageRow;
    total: number;
    limit: LimitInfo | undefined;
    onEditLimit: (t: LimitTarget) => void;
}

/** One row in a breakdown list: swatch + name, thin proportional bar, duration,
 * share, and (if a limit exists) a clickable daily-budget chip. */
export function UsageRowItem({ row, total, limit, onEditLimit }: UsageRowProps) {
    return (
        <li className="row">
            <span className="dot" style={{ background: row.color ?? "#94a3b8" }} />
            <span className="row-label">
                {row.label}
                {row.blocked && <span className="blocked-tag">blocked</span>}
            </span>
            <span className="row-bar">
                <span
                    className="row-bar-fill"
                    style={{
                        background: row.color ?? "#94a3b8",
                        width: `${percent(row.seconds, total)}%`,
                    }}
                />
            </span>
            <span className="row-time">{formatDuration(row.seconds)}</span>
            <span className="row-pct">{percent(row.seconds, total)}%</span>
            {limit && (
                <button
                    className="chip"
                    onClick={() => onEditLimit(limit.target)}
                    title={`${formatDuration(limit.defaultMinutes * 60)}/day limit`}
                >
                    {formatDuration(limit.defaultMinutes * 60)}/day
                </button>
            )}
        </li>
    );
}

/** A row in the limits list, with edit/remove actions. */
export function LimitRowItem({
    limit,
    catalog,
    onEdit,
    onRemove,
}: {
    limit: LimitInfo;
    catalog: Catalog;
    onEdit: (t: LimitTarget) => void;
    onRemove: (t: LimitTarget) => void;
}) {
    const label = targetLabelLocal(limit.target, catalog);
    return (
        <li className="row limit-row">
            <span className="row-label">{label}</span>
            <span className="row-time">{formatDuration(limit.defaultMinutes * 60)}/day</span>
            <span className={`toggle ${limit.enabled ? "on" : ""}`}>
                <span className="toggle-knob" />
            </span>
            <button className="ghost" onClick={() => onEdit(limit.target)}>
                edit
            </button>
            <button className="ghost" onClick={() => onRemove(limit.target)}>
                remove
            </button>
        </li>
    );
}

function targetLabelLocal(t: LimitTarget, catalog: Catalog): string {
    switch (t.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog.apps.find((a) => a.id === t.id)?.displayName ?? `App #${t.id}`;
        case "category":
            return catalog.categories.find((c) => c.id === t.id)?.name ?? `Category #${t.id}`;
    }
}