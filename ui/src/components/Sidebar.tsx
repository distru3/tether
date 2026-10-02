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
    /** The day picker only means something on the dashboard. */
    showDayPicker?: boolean;
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
    showDayPicker = true,
}: SidebarProps) {
    const { t } = useTranslation();
    const isLive = phase === "live";

    return (
        <aside className="app-sidebar" aria-label={t("nav.main")}>
            <div className="sidebar-brand">
                <span className="logo-title">Tether</span>
                <div className={`status-pill ${isLive ? "status-pill--live" : "status-pill--offline"}`} role="status">
                    <span className={`status-dot ${isLive ? "status-dot--live" : "status-dot--offline"}`} aria-hidden="true" />
                    <span className="status-label">{isLive ? t("sidebar.statusLive") : phase === "connecting" ? t("sidebar.statusConnecting") : t("sidebar.statusOffline")}</span>
                </div>
            </div>

            <nav className="sidebar-nav" aria-label={t("nav.sections")}>
                <button
                    type="button"
                    className={`nav-item ${activeTab === "overview" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("overview")}
                    aria-label={t("nav.overview", "Dashboard")}
                    title={t("nav.overview", "Dashboard")}
                    aria-current={activeTab === "overview" ? "page" : undefined}
                >
                    <LayoutDashboard size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.overview", "Dashboard")}</span>
                    {blockedCount > 0 && (
                        <span className="nav-badge nav-badge--danger" title={t("hero.blockedNow", { count: blockedCount })}>
                            {blockedCount}
                        </span>
                    )}
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "web-filtering" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("web-filtering")}
                    aria-label={t("nav.webFiltering", "Web Filter")}
                    title={t("nav.webFiltering", "Web Filter")}
                    aria-current={activeTab === "web-filtering" ? "page" : undefined}
                >
                    <ShieldAlert size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.webFiltering", "Web Filter")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "limits" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("limits")}
                    aria-label={t("nav.limits", "App Limits")}
                    title={t("nav.limits", "App Limits")}
                    aria-current={activeTab === "limits" ? "page" : undefined}
                >
                    <Clock size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.limits", "App Limits")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "settings" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("settings")}
                    aria-label={t("nav.settings", "Settings")}
                    title={t("nav.settings", "Settings")}
                    aria-current={activeTab === "settings" ? "page" : undefined}
                >
                    <SettingsLucide size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.settings", "Settings")}</span>
                </button>
            </nav>

            {/* Hidden rather than removed so the header keeps its balance. */}
            <div
                className={`sidebar-footer ${showDayPicker ? "" : "sidebar-footer--hidden"}`}
                aria-hidden={!showDayPicker}
            >
                <div className="day-stepper">
                    <button
                        type="button"
                        className="stepper-btn"
                        tabIndex={showDayPicker ? undefined : -1}
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
                        title={isViewingToday ? undefined : t("hero.backToToday")}
                        tabIndex={showDayPicker ? undefined : -1}
                    >
                        {!isViewingToday && <Calendar size={12} className="stepper-cal-icon" />}
                        <span>{isViewingToday ? t("sidebar.today", "Today") : formatDayLabel(viewDay)}</span>
                    </button>
                    <button
                        type="button"
                        className="stepper-btn"
                        tabIndex={showDayPicker ? undefined : -1}
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
