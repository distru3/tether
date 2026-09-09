import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { AppWindow, FolderTree } from "lucide-react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import { LiveTimer } from "./LiveTimer";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { formatDuration, sharePercent } from "../format";
import { FilterTabs } from "./FilterTabs";
import './LedgerSection.css';

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
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState("All");
    const [isExpanded, setIsExpanded] = useState(false);

    if (entries.length === 0) return null;

    const filteredEntries = entries.filter((entry) => {
        if (activeTab === "Blocked") return entry.blocked;
        if (activeTab === "Active") return !entry.blocked;
        return true;
    });

    const visibleEntries = isExpanded ? filteredEntries : filteredEntries.slice(0, 5);

    return (
        <div className="card ledger-section-card">
            <div className="ledger-card-header">
                <div className="ledger-header-text">
                    <h3 className="ledger-title">{label}</h3>
                    <p className="ledger-subtitle">{t("ledger.itemsActiveToday", { count: filteredEntries.length })}</p>
                </div>
                <FilterTabs 
                    tabs={["All", "Active", "Blocked"]} 
                    activeTab={activeTab} 
                    onChange={setActiveTab} 
                />
            </div>

            <div className="ledger-table-wrapper">
                <table className="ledger-table">
                    <thead>
                        <tr>
                            <th>{t("ledger.application", "Application")}</th>
                            <th>{t("ledger.category", "Category")}</th>
                            <th>{t("ledger.timeLimit", "Time/Limit")}</th>
                            <th>{t("ledger.progress", "Progress")}</th>
                            <th>{t("ledger.status", "Status")}</th>
                            <th>{t("ledger.action", "Action")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {visibleEntries.map((entry) => {
                            const limit = limitFor(kind, entry.id);
                            const target: LimitTargetDto =
                                kind === "app" ? { kind: "app", id: entry.id } : { kind: "category", id: entry.id };
                            const limitSeconds = entry.limit_seconds;
                            const overLimit = limitSeconds !== null && entry.seconds >= limitSeconds;
                            const share = total > 0 ? (entry.seconds / total) * 100 : 0;
                            const isUncat = kind === "app" && catalog && isUncategorizedApp(catalog, entry.id);

                            let categoryName = "—";
                            if (kind === "app" && catalog) {
                                const appInfo = catalog.apps.find((a) => a.id === entry.id);
                                if (appInfo && appInfo.primary_category) {
                                    const catInfo = catalog.categories.find(c => c.id === appInfo.primary_category);
                                    if (catInfo) {
                                        categoryName = catInfo.name;
                                    }
                                }
                            }

                            return (
                                <tr key={entry.id} className={entry.blocked ? "row-blocked" : ""}>
                                    <td>
                                        <div className="ledger-app-cell">
                                            <div 
                                                className="ledger-icon-box"
                                                style={{ backgroundColor: `${entry.color ?? 'var(--color-primary)'}22`, color: entry.color ?? 'var(--color-primary)' }}
                                            >
                                                {kind === "app" ? <AppWindow size={16} /> : <FolderTree size={16} />}
                                            </div>
                                            <span className="ledger-app-name">{entry.label}</span>
                                        </div>
                                    </td>
                                    <td>
                                        {kind === "app" ? (
                                            <span className="ledger-category-pill">{categoryName}</span>
                                        ) : (
                                            <span className="ledger-category-pill">Category Group</span>
                                        )}
                                        {isUncat && <span className="badge badge-sm badge--warning ml-2">{t("ledger.uncategorized")}</span>}
                                    </td>
                                    <td>
                                        <div className="ledger-time-cell">
                                            <span className="ledger-time">{formatDuration(entry.seconds)}</span>
                                            {limitSeconds !== null && (
                                                <span className="ledger-limit-val">/ {formatDuration(limitSeconds)}</span>
                                            )}
                                        </div>
                                    </td>
                                    <td>
                                        <div className="ledger-progress-cell">
                                            <div className="ledger-progress-track">
                                                <div 
                                                    className="ledger-progress-fill"
                                                    style={{ 
                                                        width: `${limitSeconds !== null ? limitPercent(entry.seconds, limitSeconds) : share}%`,
                                                        backgroundColor: overLimit ? "var(--color-danger)" : (entry.color ?? "var(--color-primary)")
                                                    }}
                                                />
                                            </div>
                                            <span className="ledger-progress-text font-mono">
                                                {sharePercent(entry.seconds, total)}%
                                            </span>
                                        </div>
                                    </td>
                                    <td>
                                        <div className="ledger-status-cell">
                                            {entry.blocked ? (
                                                <span className="badge badge-sm badge--danger">{t("ledger.blocked", "Blocked")}</span>
                                            ) : (
                                                <span className="badge badge-sm badge--success">{t("ledger.active", "Running")}</span>
                                            )}
                                            {entry.timer_expires_utc && <LiveTimer expiresUtc={entry.timer_expires_utc} />}
                                        </div>
                                    </td>
                                    <td>
                                        <div className="ledger-action-cell">
                                            {limit ? (
                                                <button
                                                    type="button"
                                                    className="btn btn-secondary btn-sm"
                                                    disabled={busy}
                                                    onClick={() => onEdit(limit.target, limit)}
                                                >
                                                    Edit Limit
                                                </button>
                                            ) : canLimit(entry) ? (
                                                <button
                                                    type="button"
                                                    className="btn btn-ghost btn-sm"
                                                    disabled={busy}
                                                    onClick={() => onEdit(target, null)}
                                                >
                                                    Set Limit
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
                                                    {t("ledger.tag")}
                                                </button>
                                            )}
                                        </div>
                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
                {!isExpanded && filteredEntries.length > 5 && (
                    <div className="ledger-footer">
                        <span className="ledger-footer-info">Showing 5 {kind === "app" ? "monitored applications" : "categories"}</span>
                        <button className="btn btn-ghost ledger-footer-action" onClick={() => setIsExpanded(true)}>
                            Manage all {filteredEntries.length} {kind === "app" ? "detected apps" : "categories"} &rarr;
                        </button>
                    </div>
                )}
                {isExpanded && filteredEntries.length > 5 && (
                    <div className="ledger-footer">
                        <span className="ledger-footer-info">Showing all {filteredEntries.length} {kind === "app" ? "monitored applications" : "categories"}</span>
                        <button className="btn btn-ghost ledger-footer-action" onClick={() => setIsExpanded(false)}>
                            Show less &uarr;
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}
