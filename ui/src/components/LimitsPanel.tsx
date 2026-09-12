import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { AppWindow, FolderTree, Calendar as CalendarLucide, Timer } from "lucide-react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import { describeWeekdayOverrides, targetLabel, formatDuration } from "../format";
import { CalendarIcon, LimitsIcon, PlusIcon, WarningIcon } from "./icons/Icons";
import { colorForCategory } from "../categoryColors";
import { LiveTimer } from "./LiveTimer";
import { ToggleSwitch } from "./ToggleSwitch";
import { FilterTabs } from "./FilterTabs";
import { MetricCards, type MetricData } from "./MetricCards";
import './LimitsPanel.css';

interface LimitsPanelProps {
    limits: LimitDto[];
    catalog: CatalogDto | null;
    summary: DaySummaryDto | null;
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
    summary,
    busy,
    pinConfigured,
    onToggle,
    onEdit,
    onRemove,
    onCancelPending,
    onNew,
    onOpenPinSetup,
}: LimitsPanelProps) {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState("All");

    const activeLimits = limits.filter(l => l.enabled).length;
    let limitsReached = 0;
    
    const limitUsageMap = new Map<string, number>();
    const limitTimerMap = new Map<string, string>();

    if (summary) {
        limits.forEach(l => {
            if (l.target.kind === "app" || l.target.kind === "category") {
                const arr = l.target.kind === "app" ? summary.apps : summary.categories;
                const entry = arr.find((x: any) => x.id === (l.target as any).id);
                if (entry) {
                    limitUsageMap.set(`${l.target.kind}-${(l.target as any).id}`, entry.seconds);
                    if (entry.seconds >= l.default_minutes * 60) {
                        limitsReached++;
                    }
                    if (entry.timer_expires_utc) {
                        limitTimerMap.set(`${l.target.kind}-${(l.target as any).id}`, entry.timer_expires_utc);
                    }
                }
            } else if (l.target.kind === "total") {
                limitUsageMap.set(`total-0`, summary.total_seconds);
                if (summary.total_seconds >= l.default_minutes * 60) {
                    limitsReached++;
                }
            }
        });
    }

    const metrics: MetricData[] = [
        { label: t("limits.totalLimits", "Total Limits"), value: limits.length, icon: <Timer size={16} /> },
        { label: t("limits.activeLimits", "Active Limits"), value: activeLimits, icon: <Timer size={16} /> },
        { label: t("limits.limitsReached", "Limits Reached"), value: limitsReached, icon: <WarningIcon size={16} color="var(--color-danger)" />, badge: limitsReached > 0 ? "Action Needed" : undefined }
    ];

    const filteredLimits = limits.filter(l => {
        if (activeTab === "Active") return l.enabled;
        if (activeTab === "Disabled") return !l.enabled;
        return true;
    });

    return (
        <div className="limits-container view-container limits-page-shell">
            <div className="view-header page-intro">
                <div>
                    <h2 className="view-title">{t("limits.title")}</h2>
                    <p className="view-subtitle">{t("limits.subtitle")}</p>
                </div>
                <button type="button" className="btn btn-primary btn-sm" disabled={busy} onClick={onNew}>
                    <PlusIcon size={14} />
                    {t("limits.addNewLimit")}
                </button>
            </div>

            <MetricCards metrics={metrics} />

            {!pinConfigured && (
                <div className="glass-card banner-warning-card" style={{ marginBottom: 24 }}>
                    <div className="card-icon-wrapper card-icon--amber">
                        <WarningIcon size={18} color="var(--accent-amber)" />
                    </div>
                    <div className="banner-content">
                        <strong>{t("limits.unprotected")}</strong>
                        <p>{t("limits.unprotectedDesc")}</p>
                    </div>
                    <button type="button" className="btn btn-secondary btn-sm" onClick={onOpenPinSetup}>
                        {t("limits.setPin")}
                    </button>
                </div>
            )}

            <div className="limits-list-header">
                <div>
                    <span className="section-kicker">Guardrails</span>
                    <h3 className="section-title">Your limits</h3>
                </div>
                <FilterTabs tabs={["All", "Active", "Disabled"]} activeTab={activeTab} onChange={setActiveTab} />
            </div>

            <div className="rich-limits-list">
                {filteredLimits.length === 0 && limits.length === 0 ? (
                    <div className="glass-card empty-card">
                        <LimitsIcon size={32} color="var(--text-muted)" />
                        <p dangerouslySetInnerHTML={{ __html: t("limits.noLimitsEmpty") }} />
                    </div>
                ) : (
                    filteredLimits.map((limit) => {
                        const varies = describeWeekdayOverrides(limit.weekday_minutes);
                        const label = targetLabel(limit.target, catalog);
                        const isCategory = limit.target.kind === "category";
                        const isApp = limit.target.kind === "app";
                        const targetId = limit.target.kind === "total" ? 0 : (limit.target as any).id;
                        const limitSeconds = limit.default_minutes * 60;
                        const usedSeconds = limitUsageMap.get(`${limit.target.kind}-${targetId}`) ?? 0;
                        const activeTimerExpiresUtc = limit.timer_expires_utc || limitTimerMap.get(`${limit.target.kind}-${targetId}`) || null;
                        const overLimit = usedSeconds >= limitSeconds;
                        const progressPercent = limitSeconds > 0 ? Math.min(100, (usedSeconds / limitSeconds) * 100) : 100;

                        let categoryName: string | null = null;
                        let categoryColor: string = "var(--color-primary)";

                        if (isCategory) {
                            const catObj = catalog?.categories.find(c => c.id === targetId);
                            categoryName = catObj?.name ?? label;
                            categoryColor = colorForCategory(categoryName, catObj?.color);
                        } else if (isApp) {
                            const appObj = catalog?.apps.find(a => a.id === targetId);
                            const catObj = appObj ? catalog?.categories.find(c => c.id === appObj.primary_category) : null;
                            categoryName = catObj?.name ?? "Uncategorized";
                            categoryColor = colorForCategory(categoryName, catObj?.color);
                        }

                        return (
                            <div className={`glass-card rich-limit-card ${limit.enabled ? "" : "limit-card--disabled"} ${activeTimerExpiresUtc ? "limit-card--extended" : ""}`} key={limit.id}>
                                <div className="rich-limit-card-header">
                                    <div 
                                        className="rich-limit-icon-box" 
                                        style={{ 
                                            backgroundColor: `${categoryColor}24`, 
                                            color: categoryColor,
                                            borderColor: `${categoryColor}44`,
                                        }}
                                    >
                                        {isCategory ? <FolderTree size={20} /> : <AppWindow size={20} />}
                                    </div>
                                    <div className="rich-limit-title-group">
                                        <h3 className="limit-target-name">{label}</h3>
                                        <div className="limit-badges">
                                            {isCategory ? (
                                                <span 
                                                    className="target-badge target-badge--category"
                                                    style={{ 
                                                        backgroundColor: `${categoryColor}22`, 
                                                        color: categoryColor, 
                                                        borderColor: `${categoryColor}44` 
                                                    }}
                                                >
                                                    {t("limits.category")}
                                                </span>
                                            ) : (
                                                <>
                                                    <span className="target-badge target-badge--app">
                                                        {t("limits.app")}
                                                    </span>
                                                    {categoryName && (
                                                        <span 
                                                            className="target-badge target-badge--category-tag"
                                                            style={{ 
                                                                backgroundColor: `${categoryColor}22`, 
                                                                color: categoryColor, 
                                                                borderColor: `${categoryColor}44` 
                                                            }}
                                                        >
                                                            {categoryName}
                                                        </span>
                                                    )}
                                                </>
                                            )}
                                            {overLimit && !activeTimerExpiresUtc && <span className="badge badge-sm badge--danger">Limit Reached</span>}
                                            {activeTimerExpiresUtc && (
                                                <LiveTimer expiresUtc={activeTimerExpiresUtc} showLabel label="+15m" />
                                            )}
                                        </div>
                                    </div>
                                    <div className="rich-limit-toggle">
                                        <ToggleSwitch 
                                            checked={limit.enabled} 
                                            onChange={(val) => onToggle(limit, val)} 
                                            disabled={busy}
                                        />
                                    </div>
                                </div>

                                <div className="rich-limit-card-body">
                                    <div className="rich-limit-progress">
                                        <div className="rich-limit-progress-header">
                                            <span className="rich-limit-usage">{formatDuration(usedSeconds)} <span className="rich-limit-total">/ {formatDuration(limitSeconds)}</span></span>
                                        </div>
                                        <div className="rich-limit-progress-track">
                                            <div 
                                                className="rich-limit-progress-fill" 
                                                style={{ 
                                                    width: `${progressPercent}%`, 
                                                    backgroundColor: overLimit ? 'var(--color-danger)' : categoryColor 
                                                }}
                                            />
                                        </div>
                                    </div>
                                    {activeTimerExpiresUtc && (
                                        <div className="limit-card-extension-banner">
                                            <div className="extension-banner-info">
                                                <span className="extension-pulse-dot" />
                                                <Timer size={14} className="extension-banner-icon" />
                                                <span className="extension-banner-title">{t("limits.extensionActive", "+15m Extension Active")}</span>
                                            </div>
                                            <div className="extension-banner-countdown">
                                                <LiveTimer expiresUtc={activeTimerExpiresUtc} showPulse={false} />
                                            </div>
                                        </div>
                                    )}
                                </div>

                                <div className="rich-limit-card-footer">
                                    <div className="limit-schedule-tag" title={varies ? `Per-day: ${varies}` : undefined}>
                                        <CalendarLucide size={14} color="var(--text-muted)" />
                                        <span>{varies ? t("limits.weekdayOverridesActive") : t("limits.perDay", "Per Day")}</span>
                                    </div>
                                    <div className="limit-footer-actions">
                                        <button
                                            type="button"
                                            className="btn btn-secondary btn-sm"
                                            disabled={busy}
                                            onClick={() => onEdit(limit.target, limit)}
                                        >
                                            {t("limits.editBudget")}
                                        </button>
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm text-danger"
                                            disabled={busy}
                                            onClick={() => onRemove(limit.target)}
                                        >
                                            {t("limits.remove")}
                                        </button>
                                    </div>
                                </div>
                            </div>
                        );
                    })
                )}
                {catalog?.pending_limits.map((pending) => {
                    const label = targetLabel(pending.target, catalog);
                    const isDelete = pending.action === "delete";
                    const formattedTime = new Date(pending.effective_from_utc).toLocaleString();

                    return (
                        <div className="glass-card rich-limit-card limit-card--pending" key={`pending-${pending.id}`}>
                            <div className="rich-limit-card-header">
                                <div className="rich-limit-icon-box" style={{ backgroundColor: "rgba(220, 160, 109, 0.15)", color: "var(--color-warning)" }}>
                                    <WarningIcon size={20} color="var(--color-warning)" />
                                </div>
                                <div className="rich-limit-title-group">
                                    <h3 className="limit-target-name">{label}</h3>
                                    <div className="limit-badges">
                                        <span className="target-badge" style={{ backgroundColor: "rgba(220, 160, 109, 0.2)", color: "var(--color-accent)" }}>
                                            {t("limits.pending")}
                                        </span>
                                    </div>
                                </div>
                            </div>
                            <div className="rich-limit-card-body">
                                <p style={{ fontSize: "13px", color: "var(--text-muted)" }}>
                                    {isDelete ? t("limits.scheduledForRemoval") : t("limits.changesApplyAt")}
                                    <br />
                                    <strong>{formattedTime}</strong>
                                </p>
                            </div>
                            <div className="rich-limit-card-footer">
                                <button
                                    type="button"
                                    className="btn btn-secondary btn-sm text-danger"
                                    disabled={busy}
                                    onClick={() => onCancelPending(pending.target)}
                                >
                                    {t("limits.cancelPendingChange")}
                                </button>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
