import React from "react";
import type { StatusDto } from "../types/generated/StatusDto";
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    FocusIcon,
    LimitsIcon,
    OverviewIcon,
    SettingsIcon,
    ShieldIcon,
} from "./icons/Icons";

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
}: SidebarProps) {
    const isLive = phase === "live";

    return (
        <aside className="app-sidebar" aria-label="Main Navigation">
            <div className="sidebar-brand">
                <div className="brand-logo">
                    <img src="/app-icon.png" alt="Screentime" className="brand-app-icon" />
                    <span className="logo-title">Screentime</span>
                </div>
                <div className="daemon-status" title={`Daemon status: ${phase}`}>
                    <span className={`status-dot ${isLive ? "status-dot--live" : "status-dot--offline"}`} />
                    <span className="status-label">{isLive ? "Active" : phase === "connecting" ? "Connecting..." : "Offline"}</span>
                </div>
            </div>

            <nav className="sidebar-nav">
                <button
                    type="button"
                    className={`nav-item ${activeTab === "overview" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("overview")}
                >
                    <OverviewIcon size={18} className="nav-icon" />
                    <span className="nav-text">Overview</span>
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
                    <ShieldIcon size={18} className="nav-icon" />
                    <span className="nav-text">Web Filtering</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "limits" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("limits")}
                >
                    <LimitsIcon size={18} className="nav-icon" />
                    <span className="nav-text">App Limits</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "settings" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("settings")}
                >
                    <SettingsIcon size={18} className="nav-icon" />
                    <span className="nav-text">Settings</span>
                </button>
            </nav>

            <div className="sidebar-footer">
                <div className="sidebar-info-box" style={{ padding: '12px 10px', marginBottom: '16px', borderRadius: 'var(--radius-sm)', backgroundColor: 'var(--bg-card-elevated)', border: '1px solid var(--border-subtle)' }}>
                    <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--text-secondary)', marginBottom: '4px' }}>System Policy</div>
                    <div style={{ fontSize: '11.5px', color: 'var(--text-muted)' }}>
                        {activeLimitsCount} {activeLimitsCount === 1 ? 'limit' : 'limits'} actively enforced
                    </div>
                </div>

                <div className="day-stepper">
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goPrevDay}
                        title="Previous day (Left Arrow)"
                        aria-label="Previous day"
                    >
                        <ChevronLeftIcon size={14} />
                    </button>
                    <div className="stepper-label" onClick={goToday} title="Click to jump to Today">
                        {isViewingToday ? (
                            <span className="label-today">Today</span>
                        ) : (
                            <span className="label-past">{viewDay}</span>
                        )}
                    </div>
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goNextDay}
                        disabled={isViewingToday}
                        title="Next day (Right Arrow)"
                        aria-label="Next day"
                    >
                        <ChevronRightIcon size={14} />
                    </button>
                </div>
            </div>
        </aside>
    );
}
