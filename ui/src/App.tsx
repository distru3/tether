import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Clock, Flame, Hourglass, ShieldCheck, ChevronDown } from "lucide-react";
import { applyLanguage } from "./i18n";
import { formatDayLabel, formatDuration } from "./format";
import { MetricCards } from "./components/MetricCards";
import { ExecutiveHeader } from "./components/ExecutiveHeader";
import { UsageAside } from "./components/UsageAside";
import { CategoryMix } from "./components/CategoryMix";

import { BlockedBanner } from "./components/BlockedBanner";
import { CategorizeDialog } from "./components/CategorizeDialog";
import { AppDirectoryDialog } from "./components/AppDirectoryDialog";
import { Hero } from "./components/Hero";
import { LedgerRule } from "./components/LedgerRule";
import { LimitEditorDialog } from "./components/LimitEditorDialog";
import { LimitsPanel } from "./components/LimitsPanel";
import { OnboardingSlider } from "./components/OnboardingSlider";
import { PinGate } from "./components/PinGate";
import { PinSetupDialog } from "./components/PinSetupDialog";
import { Toasts } from "./components/Toasts";
import { TutorialContent } from "./components/TutorialContent";
import { WeeklyChart } from "./components/WeeklyChart";
import { WebFilteringPanel } from "./components/WebFilteringPanel";
import { LoadingSpinner } from "./components/LoadingSpinner";
import { Sidebar, type TabKey } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { BlockOverlay } from "./components/BlockOverlay";
import { VolumeIcon } from "./components/icons/Icons";
import { previewAlertSound } from "./api";
import { useDashboard } from "./hooks/useDashboard";
import { useLedgerActions } from "./hooks/useLedgerActions";
import { useNowMinute } from "./hooks/useNowMinute";
import { useTheme } from "./hooks/useTheme";
import { useToasts } from "./hooks/useToasts";
import type { LimitDto } from "./types/generated/LimitDto";

const FIRST_RUN_KEY = "screentime_first_run_completed";

import { getCurrentWindow } from "@tauri-apps/api/window";

export function App() {
  // Ensure theme is active and synced across all windows (dashboard & overlay)
  useTheme();

  let isOverlay = window.location.search.includes("view=overlay");
  try {
    if (!isOverlay && getCurrentWindow().label === "overlay") {
      isOverlay = true;
    }
  } catch {
    // Non-Tauri fallback
  }

  if (isOverlay) {
    return <BlockOverlay />;
  }
  return <MainDashboard />;
}

function MainDashboard() {
  const { t, i18n } = useTranslation();
  const {
    phase,
    statusInfo,
    summary,
    catalog,
    lastError,
    refreshCatalog,
    refreshStatus,
    viewDay,
    isViewingToday,
    week,
    weekLoading,
    setViewDay,
    goPrevDay,
    goNextDay,
    goToday,
  } = useDashboard();
  const now = useNowMinute();
  const { theme, setTheme } = useTheme();
  const { toasts, push, dismiss } = useToasts();
  const actions = useLedgerActions({
    catalog,
    pinConfigured: statusInfo?.pin_configured ?? false,
    notify: push,
    invalidate: refreshCatalog,
    refreshStatus,
  });

  const [activeTab, setActiveTab] = useState<TabKey>("overview");
  const [appDirectoryOpen, setAppDirectoryOpen] = useState(false);
  const [pendingSettings, setPendingSettings] = useState<Record<string, boolean>>({});
  const [isRecordingHotkey, setIsRecordingHotkey] = useState(false);
  const [isPlayingChime, setIsPlayingChime] = useState(false);
  const [localVolume, setLocalVolume] = useState<number | null>(null);

  const currentVolume = localVolume ?? (statusInfo?.alert_volume !== undefined ? Number(statusInfo.alert_volume) : 80);

  const handlePreviewChime = async () => {
    try {
      setIsPlayingChime(true);
      await previewAlertSound(currentVolume);
      setTimeout(() => setIsPlayingChime(false), 850);
    } catch {
      setIsPlayingChime(false);
    }
  };

  const handleHotkeyKeyDown = (e: React.KeyboardEvent) => {
    if (!isRecordingHotkey) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      setIsRecordingHotkey(false);
      return;
    }

    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      return;
    }

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");

    let keyName = e.key.toUpperCase();
    if (e.key === " ") keyName = "Space";
    else if (e.key === "ArrowUp") keyName = "Up";
    else if (e.key === "ArrowDown") keyName = "Down";
    else if (e.key === "ArrowLeft") keyName = "Left";
    else if (e.key === "ArrowRight") keyName = "Right";

    parts.push(keyName);
    const shortcut = parts.join("+");

    setIsRecordingHotkey(false);
    void handleUpdateSetting("hud_peek_hotkey", shortcut);
  };

  async function handleUpdateSetting(key: string, value: string) {
    setPendingSettings((prev) => ({ ...prev, [key]: true }));
    try {
      await actions.setSetting(key, value);
    } catch {
      // handled
    } finally {
      setPendingSettings((prev) => ({ ...prev, [key]: false }));
    }
  }

  // First-run onboarding state
  const [showOnboarding, setShowOnboarding] = useState(() => {
    try {
      return localStorage.getItem(FIRST_RUN_KEY) !== "true";
    } catch {
      return true; // fail-safe: show onboarding if we can't read storage
    }
  });

  function handleOnboardingComplete() {
    try {
      localStorage.setItem(FIRST_RUN_KEY, "true");
    } catch {
      // ignore storage errors
    }
    setShowOnboarding(false);
  }

  const selectTab = (tab: TabKey) => {
    if (!document.startViewTransition) {
      setActiveTab(tab);
    } else {
      document.startViewTransition(() => setActiveTab(tab));
    }
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "SELECT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }
      if (event.key === "ArrowLeft") goPrevDay();
      else goNextDay();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [goPrevDay, goNextDay]);

  const loading = phase === "connecting" && summary === null;
  const total = summary?.total_seconds ?? 0;
  const categories = (summary?.categories ?? []).filter((row) => row.seconds > 0);
  const apps = (summary?.apps ?? []).filter((row) => row.seconds > 0);
  const blocked = (summary?.apps ?? []).filter((row) => row.blocked);
  const blockedCount = blocked.length;

  const pastDayEmpty =
    !isViewingToday &&
    summary !== null &&
    summary.total_seconds === 0 &&
    week !== null &&
    week.days.some((day) => day.day === viewDay && day.total_seconds === 0);

  function limitFor(kind: "app" | "category", id: number): LimitDto | undefined {
    return catalog?.limits.find((limit) => limit.target.kind === kind && limit.target.id === id);
  }

  const [focusActive, setFocusActive] = useState(false);
  const [tutorialOpen, setTutorialOpen] = useState(false);

  const activeLimitsCount = catalog ? catalog.limits.filter(l => l.enabled).length : 0;

  // Real telemetry calculations for overview metric cards
  const intervals = summary?.intervals ?? [];
  let maxIntervalSeconds = 0;
  let maxIntervalAppId = 0;
  for (const interval of intervals) {
    if (interval.durationSeconds > maxIntervalSeconds) {
      maxIntervalSeconds = interval.durationSeconds;
      maxIntervalAppId = interval.appId;
    }
  }
  const maxApp = apps.find((a) => a.id === maxIntervalAppId);
  const maxAppName = maxApp?.label ?? apps[0]?.label ?? null;
  const streakBadge = maxIntervalSeconds >= 2700 ? "Deep Focus" : maxIntervalSeconds >= 1200 ? "Steady Flow" : maxIntervalSeconds > 0 ? "Active" : undefined;
  const streakSubtitle = maxIntervalSeconds > 0 && maxAppName
    ? `Longest stretch in ${maxAppName}`
    : "No focus runs recorded";

  const weekDays = week?.days ?? [];
  const validWeekDays = weekDays.filter((d) => d.total_seconds > 0);
  const avgSeconds = validWeekDays.length > 0
    ? Math.round(validWeekDays.reduce((acc, d) => acc + d.total_seconds, 0) / validWeekDays.length)
    : total;
  const deltaPercent = avgSeconds > 0 ? Math.round(((total - avgSeconds) / avgSeconds) * 100) : 0;
  const vsAvgText = validWeekDays.length > 1
    ? deltaPercent === 0
      ? "Right on 7-day avg"
      : deltaPercent > 0
        ? `+${deltaPercent}% vs 7-day avg`
        : `${deltaPercent}% vs 7-day avg`
    : "Baseline day";

  const enabledLimits = catalog?.limits.filter((l) => l.enabled) ?? [];
  let totalBudgetSeconds = 0;
  let totalUsedOnLimitsSeconds = 0;
  for (const lim of enabledLimits) {
    totalBudgetSeconds += lim.default_minutes * 60;
    if (lim.target.kind === "app") {
      const match = apps.find((a) => a.id === (lim.target as any).id);
      if (match) totalUsedOnLimitsSeconds += match.seconds;
    } else if (lim.target.kind === "category") {
      const match = categories.find((c) => c.id === (lim.target as any).id);
      if (match) totalUsedOnLimitsSeconds += match.seconds;
    } else if (lim.target.kind === "total") {
      totalUsedOnLimitsSeconds += total;
    }
  }
  const remainingBudgetSeconds = Math.max(0, totalBudgetSeconds - totalUsedOnLimitsSeconds);

  // Show onboarding overlay on first run
  if (showOnboarding) {
    return <OnboardingSlider onComplete={handleOnboardingComplete} />;
  }

  return (
    <div className="app-root">
      <TitleBar />
      <div className="app-layout">
        <Sidebar
          activeTab={activeTab}
          onSelectTab={selectTab}
          phase={phase}
          statusInfo={statusInfo}
          viewDay={viewDay}
          isViewingToday={isViewingToday}
          goPrevDay={goPrevDay}
          goNextDay={goNextDay}
          goToday={goToday}
          blockedCount={blockedCount}
          focusActive={false}
          activeLimitsCount={activeLimitsCount}
        />

        <Toasts toasts={toasts} dismiss={dismiss} />

        <div className="app-main-content">
          <main className="sheet">

            {phase === "live" && statusInfo !== null && !statusInfo.tracking_available && (
              <p className="notice">
                {t("common.trackingUnavailable")}
              </p>
            )}
            {phase === "offline" && lastError !== null && (
              <p className="notice notice--dim">{t("common.offline")} - {lastError}</p>
            )}

            {activeTab === "overview" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)", display: "flex", flexDirection: "column", gap: "20px" }}>
                <ExecutiveHeader
                  summary={summary}
                  loading={loading}
                  total={total}
                  apps={apps}
                  viewDay={viewDay}
                  isViewingToday={isViewingToday}
                  goPrevDay={goPrevDay}
                  goNextDay={goNextDay}
                  goToday={goToday}
                  maxIntervalSeconds={maxIntervalSeconds}
                  maxAppName={maxAppName}
                  streakBadge={streakBadge}
                  streakSubtitle={streakSubtitle}
                  enabledLimits={enabledLimits}
                  remainingBudgetSeconds={remainingBudgetSeconds}
                  blockedCount={blockedCount}
                  blocked={blocked}
                  vsAvgText={vsAvgText}
                />

                {blocked.length > 0 && (
                  <BlockedBanner
                    blocked={blocked}
                    busy={actions.busy}
                    onOverride={actions.override}
                    catalog={catalog}
                  />
                )}
                
                <div className="activity-ledger-container">
                  <div className="activity-ledger-timeline">
                    <div className="timeline-panel__header">
                      <div>
                        <p className="panel-eyebrow">Today at a glance</p>
                        <h3>Daily Timeline</h3>
                      </div>
                      <div className="date-picker-placeholder">
                        <button className="btn btn-ghost btn-sm" onClick={goPrevDay} aria-label={t("hero.prevDay", "Previous Day")}>&lt;</button>
                        <span>{isViewingToday ? "Today" : formatDayLabel(viewDay!)}</span>
                        <button className="btn btn-ghost btn-sm" onClick={goNextDay} disabled={isViewingToday} aria-label={t("hero.nextDay", "Next Day")}>&gt;</button>
                      </div>
                    </div>
                    {pastDayEmpty ? (
                      <p className="empty-day">{t("hero.noUsageDay")}</p>
                    ) : (
                      <LedgerRule summary={summary} loading={loading} now={now} catalog={catalog} isToday={isViewingToday} />
                    )}
                  </div>
                  <div className="activity-ledger-aside">
                    <UsageAside
                      entries={apps}
                      total={total}
                      catalog={catalog}
                      onCategorize={actions.openCategorize}
                      onOpenAppDirectory={() => setAppDirectoryOpen(true)}
                    />
                  </div>
                </div>

                <div className="analytics-trends-container">
                  <div className="analytics-trends-history">
                    <h3 className="analytics-trends-history-title">Weekly History</h3>
                    <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                  </div>
                  <div className="analytics-trends-distribution">
                    <CategoryMix categories={categories} total={total} />
                  </div>
                </div>
              </div>
            )}

            {activeTab === "limits" && (
              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <LimitsPanel
                  limits={catalog ? catalog.limits.filter(l => l.target.kind !== "total") : []}
                  catalog={catalog}
                  summary={summary}
                  busy={actions.busy}
                  pinConfigured={statusInfo?.pin_configured ?? false}
                  onToggle={actions.toggleLimit}
                  onEdit={actions.openEditor}
                  onRemove={actions.removeLimit}
                  onCancelPending={actions.cancelPendingLimit}
                  onNew={actions.startNewOrder}
                  onOpenPinSetup={actions.openPinSetup}
                  onCategorize={actions.openCategorize}
                  onOpenAppDirectory={() => setAppDirectoryOpen(true)}
                  notify={push}
                />
              </div>
            )}

            {activeTab === "web-filtering" && (
              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <WebFilteringPanel onAttempt={actions.attempt} />
              </div>
            )}

            {activeTab === "settings" && (
              <div className="tab-page settings-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>

                <div className="settings-page-intro">
                  <span className="section-kicker">Workspace</span>
                  <h2>{t("settings.title")}</h2>
                  <p>Shape how Tether tracks, protects, and presents your day.</p>
                </div>

                <div className="settings-workspace">
                  {/* General & Appearance */}
                  <section className="settings-section settings-section--general">
                    <header className="settings-section-header">
                      <h2>{t("settings.general", "General & Appearance")}</h2>
                      <div className="section-desc">{t("settings.generalDesc", "Language and visual theme preferences.")}</div>
                    </header>
                    <div className="settings-section-body">
                      {/* Language */}
                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.language")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.languageDesc")}
                          </div>
                        </div>
                        <div className="language-selector-group">
                          <button
                            type="button"
                            className={`lang-btn ${i18n.language?.startsWith("en") ? "lang-btn--active" : ""}`}
                            onClick={() => applyLanguage("en")}
                          >
                            {t("settings.english")}
                          </button>
                          <button
                            type="button"
                            className={`lang-btn ${i18n.language?.startsWith("ar") ? "lang-btn--active" : ""}`}
                            onClick={() => applyLanguage("ar")}
                          >
                            {t("settings.arabic")}
                          </button>
                        </div>
                      </div>

                      {/* Theme selector */}
                      <div className="settings-row" style={{ flexWrap: 'wrap', gap: '12px' }}>
                        <div style={{ minWidth: '180px' }}>
                          <div className="form-label">{t("settings.theme", "Color Theme")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.themeDesc", "Choose your preferred color scheme or follow your system settings.")}
                          </div>
                        </div>
                        <div className="theme-selector-group">
                          <button
                            type="button"
                            className={`theme-btn ${theme === "midnight-cobalt" ? "theme-btn--active" : ""}`}
                            onClick={() => setTheme("midnight-cobalt")}
                          >
                            <span className="theme-swatch-dot theme-swatch-dot--midnight-cobalt" />
                            {t("settings.themeMidnightCobalt", "Midnight Cobalt")}
                          </button>
                          <button
                            type="button"
                            className={`theme-btn ${theme === "slate-charcoal" ? "theme-btn--active" : ""}`}
                            onClick={() => setTheme("slate-charcoal")}
                          >
                            <span className="theme-swatch-dot theme-swatch-dot--slate-charcoal" />
                            {t("settings.themeSlateCharcoal", "Slate Charcoal")}
                          </button>
                          <button
                            type="button"
                            className={`theme-btn ${theme === "clean-titanium" ? "theme-btn--active" : ""}`}
                            onClick={() => setTheme("clean-titanium")}
                          >
                            <span className="theme-swatch-dot theme-swatch-dot--clean-titanium" />
                            {t("settings.themeCleanTitanium", "Clean Titanium")}
                          </button>
                          <button
                            type="button"
                            className={`theme-btn ${theme === "nordic-frost" ? "theme-btn--active" : ""}`}
                            onClick={() => setTheme("nordic-frost")}
                          >
                            <span className="theme-swatch-dot theme-swatch-dot--nordic-frost" />
                            {t("settings.themeNordicFrost", "Nordic Frost")}
                          </button>
                          <button
                            type="button"
                            className={`theme-btn ${theme === "system" ? "theme-btn--active" : ""}`}
                            onClick={() => setTheme("system")}
                          >
                            <span className="theme-swatch-dot theme-swatch-dot--system" />
                            {t("settings.themeSystem", "System")}
                          </button>
                        </div>
                      </div>
                    </div>
                  </section>

                  {/* Timer HUD & Overlay */}
                  <section className="settings-section settings-section--hud">
                    <header className="settings-section-header">
                      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        <h2>{t("settings.hudTitle", "Timer HUD & Overlay")}</h2>
                        <span className="badge badge-sm badge--warning">{t("common.beta", "Beta")}</span>
                      </div>
                      <div className="section-desc">{t("settings.hudSubtitle", "Floating indicator and in-game timer controls.")}</div>
                    </header>
                    <div className="settings-section-body">
                      {/* HUD overlay */}
                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.showHud")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.showHudDesc")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                          {pendingSettings["show_hud_overlay"] && <LoadingSpinner size="sm" />}
                          <input
                            type="checkbox"
                            className="toggle-switch"
                            checked={statusInfo?.show_hud_overlay ?? true}
                            disabled={pendingSettings["show_hud_overlay"]}
                            onChange={(e) => {
                              void handleUpdateSetting("show_hud_overlay", e.target.checked.toString());
                            }}
                          />
                        </div>
                      </div>

                      {/* Floating HUD in Full-Screen Games */}
                      <div className="settings-row">
                        <div className="form-group" style={{ marginBottom: 0, flex: 1, paddingRight: '16px' }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                            <div className="form-label">{t("settings.showHudInFullscreen")}</div>
                            <span className="badge badge-sm badge--warning">{t("common.beta", "Beta")}</span>
                          </div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.showHudInFullscreenDesc")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                          {pendingSettings["show_hud_in_fullscreen"] && <LoadingSpinner size="sm" />}
                          <input
                            type="checkbox"
                            className="toggle-switch"
                            checked={statusInfo?.show_hud_in_fullscreen ?? false}
                            disabled={pendingSettings["show_hud_in_fullscreen"] || !(statusInfo?.show_hud_overlay ?? true)}
                            onChange={(e) => {
                              void handleUpdateSetting("show_hud_in_fullscreen", e.target.checked.toString());
                            }}
                          />
                        </div>
                      </div>

                      {/* Game HUD Peek Shortcut */}
                      <div className="settings-row">
                        <div className="form-group" style={{ marginBottom: 0, flex: 1, paddingRight: '16px' }}>
                          <div className="form-label">{t("settings.hudPeekShortcut")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.hudPeekShortcutDesc")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          {pendingSettings["hud_peek_hotkey"] && <LoadingSpinner size="sm" />}
                          <button
                            type="button"
                            className={`btn ${isRecordingHotkey ? 'btn-primary' : 'btn-secondary'}`}
                            style={{ minWidth: 110, fontFamily: 'var(--font-mono)', fontSize: 13, padding: '6px 12px' }}
                            onClick={() => setIsRecordingHotkey((prev) => !prev)}
                            onKeyDown={handleHotkeyKeyDown}
                            onBlur={() => setIsRecordingHotkey(false)}
                          >
                            {isRecordingHotkey ? t("settings.pressKeys") : (statusInfo?.hud_peek_hotkey || "Ctrl+Alt+T")}
                          </button>
                          {(statusInfo?.hud_peek_hotkey && statusInfo.hud_peek_hotkey !== "Ctrl+Alt+T") && (
                            <button
                              type="button"
                              className="btn btn-ghost"
                              style={{ padding: '6px 8px', fontSize: 12 }}
                              title={t("settings.resetDefault")}
                              onClick={() => void handleUpdateSetting("hud_peek_hotkey", "Ctrl+Alt+T")}
                            >
                              {t("common.reset")}
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  </section>

                  {/* Alert Sounds & Milestone Chimes */}
                  <section className="settings-section settings-section--alerts">
                    <header className="settings-section-header">
                      <h2>{t("settings.alertsTitle", "Alert Sounds & Milestone Chimes")}</h2>
                      <div className="section-desc">{t("settings.alertsSubtitle", "Studio-grade audio chimes and milestone notifications as limits approach.")}</div>
                    </header>
                    <div className="settings-section-body">
                      {/* Countdown Milestones */}
                      <div className="settings-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '8px' }}>
                        <div>
                          <div className="form-label">{t("settings.milestones", "Milestone Countdown Alerts")}</div>
                          <div className="form-hint" style={{ marginTop: 2 }}>
                            {t("settings.milestonesDesc", "Tether automatically plays a soothing harmonic chime and surfaces a 4-second timer peek when remaining screen time reaches key thresholds: 15m, 10m, 5m, and 1m, and when a limit is reached.")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px', marginTop: 4 }}>
                          {[
                            { label: "15 min", color: "var(--accent-indigo)", bg: "rgba(99, 102, 241, 0.10)", border: "rgba(99, 102, 241, 0.25)" },
                            { label: "10 min", color: "var(--accent-indigo)", bg: "rgba(99, 102, 241, 0.10)", border: "rgba(99, 102, 241, 0.25)" },
                            { label: "5 min", color: "var(--accent-amber)", bg: "rgba(245, 158, 11, 0.10)", border: "rgba(245, 158, 11, 0.25)" },
                            { label: "1 min", color: "var(--accent-rose)", bg: "rgba(244, 63, 94, 0.10)", border: "rgba(244, 63, 94, 0.25)" },
                            { label: t("common.blocked", "Limit Reached"), color: "var(--accent-rose)", bg: "rgba(244, 63, 94, 0.15)", border: "rgba(244, 63, 94, 0.35)" }
                          ].map((m) => (
                            <span
                              key={m.label}
                              style={{
                                display: 'inline-flex',
                                alignItems: 'center',
                                gap: '6px',
                                padding: '4px 10px',
                                borderRadius: '6px',
                                fontSize: '12px',
                                fontWeight: 500,
                                fontFamily: 'var(--font-mono)',
                                backgroundColor: m.bg,
                                color: m.color,
                                border: `1px solid ${m.border}`,
                              }}
                            >
                              <span style={{ width: 6, height: 6, borderRadius: '50%', backgroundColor: m.color }} />
                              {m.label}
                            </span>
                          ))}
                        </div>
                      </div>

                      {/* Alert Volume Slider */}
                      <div className="settings-row">
                        <div style={{ flex: 1, paddingRight: '24px' }}>
                          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                            <div className="form-label">{t("settings.alertVolume", "Alert Volume")}</div>
                            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                              {pendingSettings["alert_volume"] && <LoadingSpinner size="xs" />}
                              <span className="font-mono" style={{ fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', minWidth: '42px', textAlign: 'right' }}>
                                {currentVolume}%
                              </span>
                            </div>
                          </div>
                          <div className="form-hint" style={{ marginTop: 0, marginBottom: 8 }}>
                            {t("settings.alertVolumeDesc", "Adjust the volume level of milestone warnings and overlay chimes.")}
                          </div>
                          <div style={{ display: 'flex', alignItems: 'center', gap: '12px', width: '100%' }}>
                            <VolumeIcon size={16} color="var(--text-muted)" style={{ flexShrink: 0 }} />
                            <input
                              type="range"
                              min="0"
                              max="100"
                              step="5"
                              value={currentVolume}
                              disabled={pendingSettings["alert_volume"]}
                              onChange={(e) => {
                                const newVol = parseInt(e.target.value, 10);
                                setLocalVolume(newVol);
                              }}
                              onMouseUp={(e) => {
                                const newVol = parseInt((e.target as HTMLInputElement).value, 10);
                                void handleUpdateSetting("alert_volume", newVol.toString());
                              }}
                              onTouchEnd={(e) => {
                                const newVol = parseInt((e.target as HTMLInputElement).value, 10);
                                void handleUpdateSetting("alert_volume", newVol.toString());
                              }}
                              onKeyUp={(e) => {
                                const newVol = parseInt((e.target as HTMLInputElement).value, 10);
                                void handleUpdateSetting("alert_volume", newVol.toString());
                              }}
                              style={{ flex: 1, accentColor: 'var(--color-primary)', cursor: 'pointer' }}
                              aria-label={t("settings.alertVolume", "Alert Volume")}
                            />
                          </div>
                        </div>
                      </div>

                      {/* Auditory Chime Preview */}
                      <div className="settings-row">
                        <div style={{ flex: 1, paddingRight: '16px' }}>
                          <div className="form-label">{t("settings.previewChime", "Preview Chime Sound")}</div>
                          <div className="form-hint" style={{ marginTop: 2 }}>
                            {t("settings.chimeDesignNote", "Gentle, non-intrusive harmonic chime designed to never startle or clash with game audio.")}
                          </div>
                        </div>
                        <button
                          type="button"
                          className="btn btn-secondary"
                          style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            gap: '8px',
                            padding: '7px 16px',
                            minWidth: '140px',
                            justifyContent: 'center',
                            transition: 'all 0.15s ease'
                          }}
                          disabled={isPlayingChime}
                          onClick={() => void handlePreviewChime()}
                        >
                          <VolumeIcon size={16} color={isPlayingChime ? "var(--accent-indigo)" : "currentColor"} />
                          {isPlayingChime ? t("settings.previewChimePlaying", "Playing…") : t("settings.previewChime", "Preview Chime")}
                        </button>
                      </div>
                    </div>
                  </section>

                  {/* Security & Protection */}
                  <section className="settings-section settings-section--security">
                    <header className="settings-section-header">
                      <h2>{t("settings.security")}</h2>
                      <div className="section-desc">{t("settings.securityDesc")}</div>
                    </header>
                    <div className="settings-section-body">
                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.adminPin")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {statusInfo?.pin_configured ? t("settings.pinSet") : t("settings.pinNotSet")}
                          </div>
                        </div>
                        <button
                          type="button"
                          className="btn btn-secondary"
                          onClick={actions.openPinSetup}
                        >
                          {statusInfo?.pin_configured ? t("settings.changePin") : t("settings.setPin")}
                        </button>
                      </div>

                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.strictMode", "Strict Mode")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.strictModeDesc", "Prevents circumvention by blocking task manager and registry edits while limits are active.")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                          {pendingSettings["strict_mode"] && <LoadingSpinner size="sm" />}
                          <input
                            type="checkbox"
                            className="toggle-switch"
                            checked={statusInfo?.strict_mode ?? false}
                            disabled={pendingSettings["strict_mode"]}
                            onChange={(e) => {
                              void handleUpdateSetting("strict_mode", e.target.checked.toString());
                            }}
                          />
                        </div>
                      </div>

                      <div className="settings-row">
                        <div style={{ flex: 1, paddingRight: '16px' }}>
                          <div className="form-label">{t("settings.familyDns", "Family DNS Protection")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.familyDnsDesc", "Filter adult and malicious domains system-wide using Cloudflare Family DNS. Your original network DNS settings are automatically preserved and restored when disabled.")}
                          </div>
                          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginTop: '6px', fontSize: '12px' }}>
                            <span
                              style={{
                                width: 8,
                                height: 8,
                                borderRadius: '50%',
                                backgroundColor: statusInfo?.family_dns_enabled ? '#10b981' : 'var(--text-muted)',
                                boxShadow: statusInfo?.family_dns_enabled ? '0 0 6px rgba(16, 185, 129, 0.6)' : 'none',
                                display: 'inline-block',
                              }}
                            />
                            <span style={{ color: 'var(--text-secondary)' }}>
                              {statusInfo?.family_dns_enabled
                                ? t("settings.familyDnsActive", "Protected (Cloudflare Family)")
                                : t("settings.familyDnsInactive", "Disabled (Original DNS)")}
                            </span>
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                          {pendingSettings["family_dns"] && <LoadingSpinner size="sm" />}
                          <input
                            type="checkbox"
                            className="toggle-switch"
                            checked={statusInfo?.family_dns_enabled ?? false}
                            disabled={pendingSettings["family_dns"]}
                            onChange={(e) => {
                              void handleUpdateSetting("family_dns", e.target.checked.toString());
                            }}
                          />
                        </div>
                      </div>
                    </div>
                  </section>

                  {/* System Parameters */}
                  <section className="settings-section settings-section--advanced">
                    <header className="settings-section-header">
                      <h2>{t("settings.advanced", "Advanced Parameters")}</h2>
                      <div className="section-desc">{t("settings.advancedDesc", "Fine-tune system thresholds and enforcement behaviour.")}</div>
                    </header>
                    <div className="settings-section-body">
                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.cooldown", "Anti-impulse Cooldown")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.cooldownDesc", "Time delay before a relaxed limit takes effect. Tightening applies instantly.")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          {pendingSettings["limit_cooldown_hours"] && <LoadingSpinner size="xs" />}
                          <div className="input-with-suffix">
                            <input
                              type="number"
                              className="input"
                              style={{ width: '60px' }}
                              key={`cooldown_${statusInfo?.limit_cooldown_hours}`}
                              defaultValue={statusInfo?.limit_cooldown_hours?.toString() ?? "24"}
                              disabled={pendingSettings["limit_cooldown_hours"]}
                              onBlur={(e) => {
                                if (e.target.value !== statusInfo?.limit_cooldown_hours?.toString()) {
                                  void handleUpdateSetting("limit_cooldown_hours", e.target.value);
                                }
                              }}
                            />
                            <span className="input-suffix">hrs</span>
                          </div>
                        </div>
                      </div>

                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.idleThreshold", "Idle Threshold")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.idleThresholdDesc", "Seconds without input before usage stops accruing.")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          {pendingSettings["idle_threshold_secs"] && <LoadingSpinner size="xs" />}
                          <div className="input-with-suffix">
                            <input
                              type="number"
                              className="input"
                              style={{ width: '60px' }}
                              key={`idle_${statusInfo?.idle_threshold_secs}`}
                              defaultValue={statusInfo?.idle_threshold_secs?.toString() ?? "60"}
                              disabled={pendingSettings["idle_threshold_secs"]}
                              onBlur={(e) => {
                                if (e.target.value !== statusInfo?.idle_threshold_secs?.toString()) {
                                  void handleUpdateSetting("idle_threshold_secs", e.target.value);
                                }
                              }}
                            />
                            <span className="input-suffix">sec</span>
                          </div>
                        </div>
                      </div>

                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("settings.dayReset", "Day Start Offset")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {t("settings.dayResetDesc", "Minutes after local midnight at which daily budgets reset (0 = midnight).")}
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          {pendingSettings["day_start_minutes"] && <LoadingSpinner size="xs" />}
                          <div className="input-with-suffix">
                            <input
                              type="number"
                              className="input"
                              style={{ width: '60px' }}
                              key={`day_start_${statusInfo?.day_start_minutes}`}
                              defaultValue={statusInfo?.day_start_minutes?.toString() ?? "0"}
                              disabled={pendingSettings["day_start_minutes"]}
                              onBlur={(e) => {
                                if (e.target.value !== statusInfo?.day_start_minutes?.toString()) {
                                  void handleUpdateSetting("day_start_minutes", e.target.value);
                                }
                              }}
                            />
                            <span className="input-suffix">min</span>
                          </div>
                        </div>
                      </div>
                    </div>
                  </section>

                  {/* Application Categories */}
                  <section className="settings-section settings-section--categories">
                    <header className="settings-section-header">
                      <h2>{t("settings.categoriesTitle", "Application Categories")}</h2>
                      <div className="section-desc">{t("settings.categoriesDesc", "Manage how Tether classifies and groups applications on your device.")}</div>
                    </header>
                    <div className="settings-section-body">
                      <div className="settings-row">
                        <div>
                          <div className="form-label">{t("categorize.appDirectory", "Application Directory")}</div>
                          <div className="form-hint" style={{ marginTop: 0 }}>
                            {catalog?.apps.length
                              ? t("categorize.appsDetected", { count: catalog.apps.length })
                              : t("settings.appDirectoryDesc", "Browse all detected applications on this PC and customize their categories.")}
                          </div>
                        </div>
                        <button
                          type="button"
                          className="btn btn-secondary"
                          onClick={() => setAppDirectoryOpen(true)}
                        >
                          {t("categorize.manageApps", "Manage Applications")}
                        </button>
                      </div>
                    </div>
                  </section>

                  {/* Guide & Shortcuts */}
                  <section className="settings-section settings-section--tutorial">
                    <header
                      className="settings-section-header"
                      style={{ cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: tutorialOpen ? 18 : 0 }}
                      onClick={() => setTutorialOpen(!tutorialOpen)}
                    >
                      <div>
                        <h2 style={{ margin: 0 }}>{t("tutorial.title")}</h2>
                        <div className="section-desc">Quick guide to shortcuts, hotkeys, and app controls.</div>
                      </div>
                      <ChevronDown
                        size={18}
                        style={{
                          transform: tutorialOpen ? 'rotate(180deg)' : 'rotate(0deg)',
                          transition: 'transform 0.2s ease',
                          color: 'var(--text-muted)'
                        }}
                      />
                    </header>
                    <div style={{ maxHeight: tutorialOpen ? '2000px' : '0', opacity: tutorialOpen ? 1 : 0, overflow: 'hidden', transition: 'all 0.4s ease-in-out' }}>
                      <div style={{ paddingTop: 12 }}>
                        <TutorialContent />
                      </div>
                    </div>
                  </section>

                  {/* About */}
                  <section className="settings-section settings-section--about">
                    <header className="settings-section-header" style={{ marginBottom: 8 }}>
                      <h2>{t("settings.about")}</h2>
                    </header>
                    <p className="form-hint" style={{ margin: 0 }}>{t("settings.aboutVersion")}</p>
                  </section>
                </div>
              </div>
            )}

            {actions.editor !== null && (
              <LimitEditorDialog
                catalog={catalog}
                target={actions.editor.target}
                limit={actions.editor.limit}
                busy={actions.busy}
                onClose={actions.closeEditor}
                onSubmit={actions.submitEditor}
                onCategorize={actions.openCategorize}
              />
            )}
            {actions.gate !== null && (
              <PinGate
                label={actions.gate.label}
                error={actions.gateError}
                busy={actions.busy}
                onSubmit={actions.submitGate}
                onClose={actions.cancelGate}
              />
            )}
            {actions.pinSetupOpen && (
              <PinSetupDialog
                pinConfigured={statusInfo?.pin_configured ?? false}
                busy={actions.busy}
                onClose={actions.closePinSetup}
                onSubmit={actions.submitPinSetup}
              />
            )}
            {appDirectoryOpen && (
              <AppDirectoryDialog
                catalog={catalog}
                busy={actions.busy}
                onClose={() => setAppDirectoryOpen(false)}
                onCategorize={(appId, appName, primaryId, tagIds) => {
                  actions.openCategorize(appId, appName, primaryId, tagIds);
                }}
              />
            )}
            {actions.categorizeTarget !== null && (
              <CategorizeDialog
                appName={actions.categorizeTarget.appName}
                currentPrimaryId={actions.categorizeTarget.primaryId}
                currentTagIds={actions.categorizeTarget.tagIds}
                catalog={catalog}
                busy={actions.busy}
                onClose={actions.closeCategorize}
                onCategorize={actions.submitCategorize}
                onAutoDetect={actions.resetCategorize}
              />
            )}
          </main>
        </div>
      </div>
    </div>
  );
}
