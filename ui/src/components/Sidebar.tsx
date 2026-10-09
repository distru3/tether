import { useTranslation } from "react-i18next";
import type { StatusDto } from "../types/generated/StatusDto";
import {
    LayoutDashboard,
    Clock,
    BarChart3,
    Settings as SettingsLucide,
    ChevronLeft,
    ChevronRight,
    Calendar,
} from "lucide-react";
import { formatDayLabel } from "../format";

/** Top-level pages. Websites live under Limits (docs/DESIGN_SYSTEM.md §2). */
export type TabKey = "overview" | "limits" | "activity" | "settings";

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
                <span className="brand-mark" aria-hidden="true">T</span>
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
                    aria-label={t("nav.overview")}
                    title={t("nav.overview")}
                    aria-current={activeTab === "overview" ? "page" : undefined}
                >
                    <LayoutDashboard size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.overview")}</span>
                    {blockedCount > 0 && (
                        <span className="nav-badge nav-badge--danger" title={t("hero.blockedNow", { count: blockedCount })}>
                            {blockedCount}
                        </span>
                    )}
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "limits" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("limits")}
                    aria-label={t("nav.limits")}
                    title={t("nav.limits")}
                    aria-current={activeTab === "limits" ? "page" : undefined}
                >
                    <Clock size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.limits")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "activity" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("activity")}
                    aria-label={t("nav.activity")}
                    title={t("nav.activity")}
                    aria-current={activeTab === "activity" ? "page" : undefined}
                >
                    <BarChart3 size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.activity")}</span>
                </button>

                <button
                    type="button"
                    className={`nav-item ${activeTab === "settings" ? "nav-item--active" : ""}`}
                    onClick={() => onSelectTab("settings")}
                    aria-label={t("nav.settings")}
                    title={t("nav.settings")}
                    aria-current={activeTab === "settings" ? "page" : undefined}
                >
                    <SettingsLucide size={15} className="nav-icon" />
                    <span className="nav-text">{t("nav.settings")}</span>
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
                        <span>{isViewingToday ? t("sidebar.today") : formatDayLabel(viewDay)}</span>
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
