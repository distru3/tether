import React from "react";
import { useTranslation } from "react-i18next";
import type { StatusDto } from "../types/generated/StatusDto";
import {
    LayoutDashboard,
    ShieldAlert,
    Clock,
    Settings as SettingsLucide,
    ChevronLeft,
    ChevronRight,
    Activity,
    Info,
    Clock3
} from "lucide-react";
import { formatDayLabel } from "../format";

export type TabKey = "overview" | "limits" | "web-filtering" | "settings";

interface SidebarProps {
    activeTab: TabKey;
    onSelectTab: (tab: TabKey) => void;
    phase: "connecting" | "live" | "offline";
    statusInfo: StatusDto | null;
    viewDay: number;
    isViewingToday: boolean;
    goPrevDay: () => void;
    goNextDay: () => void;
    goToday: () => void;
    blockedCount: number;
    focusActive: boolean;
    activeLimitsCount?: number;
}

export function Sidebar({
    activeTab,
    onSelectTab,
    phase,
    viewDay,
    isViewingToday,
    goPrevDay,
    goNextDay,
    goToday,
    blockedCount,
    focusActive,
    activeLimitsCount = 0,
    statusInfo,
}: SidebarProps) {
    const { t } = useTranslation();
    const isLive = phase === "live";

    return (
        <aside className="app-sidebar" aria-label="Main Navigation">
            <div className="sidebar-brand">
                <div className="brand-logo">
                    <span className="brand-mark" aria-hidden="true"><Clock3 size={17} strokeWidth={1.8} /></span>
                    <span className="logo-title">Screentime</span>
                </div>
                <div className="daemon-status" title={`Daemon status: ${phase}`}>
                    <span className={`status-dot ${isLive ? "status-dot--live" : "status-dot--offline"}`} />
                    <span className="status-label">{isLive ? "Live" : phase === "connecting" ? t("sidebar.statusConnecting") : t("sidebar.statusOffline")}</span>
                </div>
            </div>

            <div className="sidebar-nav-label">Workspace</div>
            <nav className="sidebar-nav">
                <button
                    type="button"
                    className={`nav-item ${activeTab === "overview" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("overview")}
                >
                    <LayoutDashboard size={18} className="nav-icon" />
                    <span className="nav-text">{t("nav.overview", "Dashboard")}</span>
                    {blockedCount > 0 && (
                        <span className="nav-badge nav-badge--danger" title={`${blockedCount} apps blocked`}>
                            {blockedCount}
                        </span>
                    )}
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "web-filtering" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("web-filtering")}
                >
                    <ShieldAlert size={18} className="nav-icon" />
                    <span className="nav-text">{t("nav.webFiltering", "Web Filter")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "limits" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("limits")}
                >
                    <Clock size={18} className="nav-icon" />
                    <span className="nav-text">{t("nav.limits", "App Limits")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "settings" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("settings")}
                >
                    <SettingsLucide size={18} className="nav-icon" />
                    <span className="nav-text">{t("nav.settings", "Settings")}</span>
                </button>
            </nav>

            <div className="sidebar-footer">
                <div className="sidebar-info-box" style={{ padding: '12px 10px', marginBottom: '16px', borderRadius: 'var(--radius-sm)', backgroundColor: 'var(--bg-card-elevated)', border: '1px solid var(--border-subtle)' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', fontWeight: 600, color: 'var(--text-secondary)', marginBottom: '4px' }}>
                        <Info size={14} />
                        {t("sidebar.systemPolicy", "System Policy")}
                    </div>
                    <div style={{ fontSize: '11.5px', color: 'var(--text-muted)' }}>
                        {activeLimitsCount === 1 ? t("sidebar.limitEnforced", { count: activeLimitsCount, defaultValue: "1 limit actively enforced" }) : t("sidebar.limitsEnforced", { count: activeLimitsCount, defaultValue: "{{count}} limits enforced" })}
                    </div>
                    {statusInfo && (
                        <div style={{ fontSize: '11px', color: 'var(--text-muted)', marginTop: '6px', display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <Activity size={12} color={statusInfo.tracking_available ? "var(--color-success)" : "var(--color-warning)"} />
                            {statusInfo.tracking_available ? "Tracker Active" : "Tracker Blind"}
                        </div>
                    )}
                </div>

                <div className="day-stepper" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', width: '100%' }}>
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goPrevDay}
                        title={t("sidebar.prevDay")}
                        aria-label={t("sidebar.prevDay")}
                        style={{ flexShrink: 0 }}
                    >
                        <ChevronLeft size={16} />
                    </button>
                    <div className="stepper-label" onClick={goToday} title={t("sidebar.jumpToday")} style={{ flexGrow: 1, textAlign: 'center', minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', padding: '0 4px' }}>
                        {isViewingToday ? (
                            <span className="label-today">{t("sidebar.today", "Today")}</span>
                        ) : (
                            <span className="label-past">{formatDayLabel(viewDay)}</span>
                        )}
                    </div>
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goNextDay}
                        disabled={isViewingToday}
                        title={t("sidebar.nextDay")}
                        aria-label={t("sidebar.nextDay")}
                        style={{ flexShrink: 0 }}
                    >
                        <ChevronRight size={16} />
                    </button>
                </div>
            </div>
        </aside>
    );
}
