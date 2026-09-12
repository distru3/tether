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
    Calendar,
} from "lucide-react";
import { formatDayLabel } from "../format";
import { TetherLogo } from "./TetherLogo";

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
                <div className="brand-cluster">
                    <div className="brand-logo" title="Tether">
                        <TetherLogo size={28} />
                    </div>
                    <div className="brand-title-group">
                        <span className="logo-title">Tether</span>
                        <div className={`status-pill ${isLive ? "status-pill--live" : "status-pill--offline"}`} title={`Daemon status: ${phase}`}>
                            <span className={`status-dot ${isLive ? "status-dot--live" : "status-dot--offline"}`} />
                            <span className="status-label">{isLive ? "Live" : phase === "connecting" ? t("sidebar.statusConnecting") : t("sidebar.statusOffline")}</span>
                        </div>
                    </div>
                </div>
            </div>

            <nav className="sidebar-nav" aria-label="Sections">
                <button
                    type="button"
                    className={`nav-item ${activeTab === "overview" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("overview")}
                >
                    <LayoutDashboard size={15} className="nav-icon" />
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
                    <ShieldAlert size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.webFiltering", "Web Filter")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "limits" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("limits")}
                >
                    <Clock size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.limits", "App Limits")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "settings" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("settings")}
                >
                    <SettingsLucide size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.settings", "Settings")}</span>
                </button>
            </nav>

            <div className="sidebar-footer">
                <div className="day-stepper">
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goPrevDay}
                        title={t("sidebar.prevDay")}
                        aria-label={t("sidebar.prevDay")}
                    >
                        <ChevronLeft size={15} />
                    </button>
                    <button
                        type="button"
                        className={`stepper-label-btn ${!isViewingToday ? "stepper-label-btn--past" : ""}`}
                        onClick={goToday}
                        title={isViewingToday ? "Viewing Today" : "Click to return to Today"}
                    >
                        {!isViewingToday && <Calendar size={12} className="stepper-cal-icon" />}
                        <span>{isViewingToday ? t("sidebar.today", "Today") : formatDayLabel(viewDay)}</span>
                    </button>
                    <button
                        type="button"
                        className="stepper-btn"
                        onClick={goNextDay}
                        disabled={isViewingToday}
                        title={t("sidebar.nextDay")}
                        aria-label={t("sidebar.nextDay")}
                    >
                        <ChevronRight size={15} />
                    </button>
                </div>
            </div>
        </aside>
    );
}
