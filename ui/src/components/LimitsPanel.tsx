import React from "react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { describeWeekdayOverrides, targetLabel } from "../format";
import { CalendarIcon, LimitsIcon, PlusIcon, WarningIcon } from "./icons/Icons";

interface LimitsPanelProps {
    limits: LimitDto[];
    catalog: CatalogDto | null;
    busy: boolean;
    pinConfigured: boolean;
    onToggle: (limit: LimitDto, next: boolean) => void;
    onEdit: (target: LimitTargetDto, limit: LimitDto | null) => void;
    onRemove: (target: LimitTargetDto) => void;
    onCancelPending: (target: LimitTargetDto) => void;
    onNew: () => void;
    onOpenPinSetup: () => void;
}

export function LimitsPanel({
    limits,
    catalog,
    busy,
    pinConfigured,
    onToggle,
    onEdit,
    onRemove,
    onCancelPending,
    onNew,
    onOpenPinSetup,
}: LimitsPanelProps) {
    return (
        <div className="limits-container view-container">
            <div className="view-header">
                <div>
                    <h2 className="view-title">App & Category Limits</h2>
                    <p className="view-subtitle">
                        Enforce daily time budgets on apps and categories. Loosening changes undergo an anti-impulse cooldown.
                    </p>
                </div>
                <button type="button" className="btn btn-primary btn-sm" disabled={busy} onClick={onNew}>
                    <PlusIcon size={14} />
                    Add New Limit
                </button>
            </div>

            {!pinConfigured && (
                <div className="glass-card banner-warning-card">
                    <div className="card-icon-wrapper card-icon--amber">
                        <WarningIcon size={18} color="var(--accent-amber)" />
                    </div>
                    <div className="banner-content">
                        <strong>Limits are currently unprotected.</strong>
                        <p>Set a master PIN to prevent bypass or unauthorized quota removal.</p>
                    </div>
                    <button type="button" className="btn btn-secondary btn-sm" onClick={onOpenPinSetup}>
                        Set PIN
                    </button>
                </div>
            )}

            <div className="limits-grid">
                {limits.length === 0 ? (
                    <div className="glass-card empty-card">
                        <LimitsIcon size={32} color="var(--text-muted)" />
                        <p>No limits configured. Click <strong>Add New Limit</strong> to set daily quotas on apps or categories.</p>
                    </div>
                ) : (
                    limits.map((limit) => {
                        const varies = describeWeekdayOverrides(limit.weekday_minutes);
                        const label = targetLabel(limit.target, catalog);
                        const isCategory = limit.target.kind === "category";

                        return (
                            <div className={`glass-card limit-card ${limit.enabled ? "" : "limit-card--disabled"}`} key={limit.id}>
                                <div className="limit-card-header">
                                    <div className="limit-card-title-group">
                                        <span className={`target-badge ${isCategory ? "target-badge--category" : "target-badge--app"}`}>
                                            {isCategory ? "Category" : "App"}
                                        </span>
                                        <h3 className="limit-target-name">{label}</h3>
                                    </div>
                                    <input
                                        type="checkbox"
                                        className="toggle-switch"
                                        checked={limit.enabled}
                                        disabled={busy}
                                        onChange={(e) => onToggle(limit, e.target.checked)}
                                        title={limit.enabled ? "Disable limit" : "Enable limit"}
                                    />
                                </div>

                                <div className="limit-card-body">
                                    <div className="limit-budget-row font-mono">
                                        <span className="budget-value">{limit.default_minutes}m</span>
                                        <span className="budget-unit">/ day</span>
                                    </div>
                                    {varies !== null && (
                                        <div className="limit-schedule-tag" title={`Per-day: ${varies}`}>
                                            <CalendarIcon size={13} color="var(--accent-indigo)" />
                                            <span>Weekday overrides active</span>
                                        </div>
                                    )}
                                </div>

                                <div className="limit-card-footer">
                                    <button
                                        type="button"
                                        className="btn btn-secondary btn-sm"
                                        disabled={busy}
                                        onClick={() => onEdit(limit.target, limit)}
                                    >
                                        Edit Budget
                                    </button>
                                    <button
                                        type="button"
                                        className="btn btn-ghost btn-sm text-danger"
                                        disabled={busy}
                                        onClick={() => onRemove(limit.target)}
                                    >
                                        Remove
                                    </button>
                                </div>
                            </div>
                        );
                    })
                )}
                {catalog?.pending_limits.map((pending) => {
                    const label = targetLabel(pending.target, catalog);
                    const isCategory = pending.target.kind === "category";
                    const isDelete = pending.action === "delete";
                    const formattedTime = new Date(pending.effective_from_utc).toLocaleString();

                    return (
                        <div className="glass-card limit-card limit-card--pending" key={`pending-${pending.id}`}>
                            <div className="limit-card-header">
                                <div className="limit-card-title-group">
                                    <span className="target-badge" style={{ backgroundColor: "var(--accent-amber)", color: "black" }}>
                                        Pending
                                    </span>
                                    <h3 className="limit-target-name">{label}</h3>
                                </div>
                            </div>
                            <div className="limit-card-body">
                                <p style={{ fontSize: "0.85rem", color: "var(--text-muted)", marginBottom: "8px" }}>
                                    {isDelete ? "Scheduled for removal at:" : "Changes apply at:"}
                                    <br />
                                    <strong>{formattedTime}</strong>
                                </p>
                                {!isDelete && pending.default_minutes != null && (
                                    <div className="limit-budget-row font-mono" style={{ opacity: 0.7 }}>
                                        <span className="budget-value">{pending.default_minutes}m</span>
                                        <span className="budget-unit">/ day</span>
                                    </div>
                                )}
                            </div>
                            <div className="limit-card-footer">
                                <button
                                    type="button"
                                    className="btn btn-secondary btn-sm text-danger"
                                    disabled={busy}
                                    onClick={() => onCancelPending(pending.target)}
                                >
                                    Cancel Pending Change
                                </button>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
