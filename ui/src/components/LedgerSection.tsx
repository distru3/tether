import React from "react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import { LiveTimer } from "./LiveTimer";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { formatDuration, sharePercent } from "../format";

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

    return (
        <div className="glass-card usage-section-card">
            <div className="section-card-header">
                <div>
                    <h3 className="card-title">{label}</h3>
                    <p className="card-description">{entries.length} items active today</p>
                </div>
            </div>

            <div className="usage-rows-list">
                {entries.map((entry, index) => {
                    const limit = limitFor(kind, entry.id);
                    const target: LimitTargetDto =
                        kind === "app" ? { kind: "app", id: entry.id } : { kind: "category", id: entry.id };
                    const limitSeconds = entry.limit_seconds;
                    const overLimit = limitSeconds !== null && entry.seconds >= limitSeconds;
                    const share = total > 0 ? (entry.seconds / total) * 100 : 0;
                    const isUncat = kind === "app" && catalog && isUncategorizedApp(catalog, entry.id);

                    return (
                        <div key={entry.id} className={`usage-row ${entry.blocked ? "usage-row--blocked" : ""}`}>
                            <div className="usage-row-main">
                                <span className="usage-rank font-mono">{index + 1}</span>
                                <span
                                    className="usage-swatch"
                                    style={{ backgroundColor: entry.color ?? "var(--accent-indigo)" }}
                                />
                                <div className="usage-name-group">
                                    <span className="usage-label">{entry.label}</span>
                                    {entry.blocked && <span className="badge badge-sm badge--danger">Blocked</span>}
                                    {entry.timer_expires_utc && <LiveTimer expiresUtc={entry.timer_expires_utc} />}
                                    {isUncat && <span className="badge badge-sm badge--warning">Uncategorized</span>}
                                </div>

                                <div className="usage-metrics font-mono">
                                    <span className="usage-time">{formatDuration(entry.seconds)}</span>
                                    <span className="usage-percent">{sharePercent(entry.seconds, total)}%</span>
                                </div>

                                <div className="usage-actions">
                                    {limit ? (
                                        <button
                                            type="button"
                                            className="btn btn-secondary btn-sm"
                                            disabled={busy}
                                            onClick={() => onEdit(limit.target, limit)}
                                        >
                                            {formatDuration(limit.default_minutes * 60)} limit
                                        </button>
                                    ) : canLimit(entry) ? (
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm"
                                            disabled={busy}
                                            onClick={() => onEdit(target, null)}
                                        >
                                            + Limit
                                        </button>
                                    ) : null}

                                    {kind === "app" && onCategorize && (
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm"
                                            disabled={busy}
                                            onClick={() => {
                                                const app = catalog?.apps.find((a) => a.id === entry.id);
                                                onCategorize(entry.id, entry.label, app?.primary_category ?? null, app?.tags ?? []);
                                            }}
                                        >
                                            Tag
                                        </button>
                                    )}
                                </div>
                            </div>

                            {/* Share progress bar */}
                            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', marginTop: '8px' }}>
                                {limitSeconds !== null ? (
                                    <>
                                        <div className="usage-progress-track" style={{ height: '3px', marginTop: 0 }}>
                                            <div
                                                className={`usage-progress-bar ${overLimit ? "usage-progress-bar--over" : "usage-progress-bar--limit"}`}
                                                style={{
                                                    width: `${limitPercent(entry.seconds, limitSeconds)}%`,
                                                    backgroundColor: overLimit ? "var(--color-danger)" : "var(--color-info)"
                                                }}
                                                title={`${Math.round(limitPercent(entry.seconds, limitSeconds))}% of limit`}
                                            />
                                        </div>
                                        <div className="usage-progress-track" style={{ height: '3px', marginTop: 0 }}>
                                            <div
                                                className="usage-progress-bar"
                                                style={{
                                                    width: `${share}%`,
                                                    backgroundColor: entry.color ?? "var(--accent-indigo)"
                                                }}
                                                title={`${Math.round(share)}% of day`}
                                            />
                                        </div>
                                    </>
                                ) : (
                                    <div className="usage-progress-track" style={{ height: '4px', marginTop: 0 }}>
                                        <div
                                            className="usage-progress-bar"
                                            style={{
                                                width: `${share}%`,
                                                backgroundColor: entry.color ?? "var(--accent-indigo)",
                                            }}
                                            title={`${Math.round(share)}% of day`}
                                        />
                                    </div>
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
