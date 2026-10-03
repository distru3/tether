import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ShieldOff } from "lucide-react";
import { UsageAside } from "./components/UsageAside";

import { BlockedBanner } from "./components/BlockedBanner";
import { CategorizeDialog } from "./components/CategorizeDialog";
import { AppDirectoryDialog } from "./components/AppDirectoryDialog";
import { LimitEditorDialog } from "./components/LimitEditorDialog";
import { SetupFlow } from "./components/setup/SetupFlow";
import { PinGate } from "./components/PinGate";
import { PinSetupDialog } from "./components/PinSetupDialog";
import { Toasts } from "./components/Toasts";
import { WeeklyChart } from "./components/WeeklyChart";
import { Sidebar, type TabKey } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { BlockOverlay } from "./components/BlockOverlay";
import { LimitsPage } from "./pages/LimitsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TodayPage } from "./pages/TodayPage";
import { useDashboard } from "./hooks/useDashboard";
import { useLedgerActions } from "./hooks/useLedgerActions";
import { useNowMinute } from "./hooks/useNowMinute";
import { useTheme } from "./hooks/useTheme";
import { useToasts } from "./hooks/useToasts";

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
  const { t } = useTranslation();
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
  const apps = (summary?.apps ?? []).filter((row) => row.seconds > 0);
  const blocked = (summary?.apps ?? []).filter((row) => row.blocked);
  const blockedCount = blocked.length;

  const pastDayEmpty =
    !isViewingToday &&
    summary !== null &&
    summary.total_seconds === 0 &&
    week !== null &&
    week.days.some((day) => day.day === viewDay && day.total_seconds === 0);


  const activeLimitsCount = catalog ? catalog.limits.filter(l => l.enabled).length : 0;


  // Show onboarding overlay on first run
  if (showOnboarding) {
    return <SetupFlow onComplete={handleOnboardingComplete} />;
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
          showDayPicker={activeTab === "overview"}
        />

        <Toasts toasts={toasts} dismiss={dismiss} />

        <div className="app-main-content">
          <main className="sheet">

            {phase === "live" && statusInfo !== null && !statusInfo.tracking_available && (
              <p className="notice">
                {t("common.trackingUnavailable")}
              </p>
            )}
            {phase === "offline" && (
              <div className="service-alert" role="alert">
                <ShieldOff size={18} aria-hidden="true" className="service-alert__icon" />
                <div>
                  <p className="service-alert__title">{t("offline.title")}</p>
                  <p className="service-alert__body">{t("offline.body")}</p>
                  {lastError !== null && <p className="service-alert__detail">{lastError}</p>}
                </div>
              </div>
            )}

            {activeTab === "overview" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <TodayPage
                  summary={summary}
                  catalog={catalog}
                  statusInfo={statusInfo}
                  viewDay={viewDay}
                  isViewingToday={isViewingToday}
                  loading={loading}
                  now={now}
                  actions={actions}
                  onOpenLimits={() => selectTab("limits")}
                  banner={
                    blocked.length > 0 && (
                      <BlockedBanner
                        blocked={blocked}
                        busy={actions.busy}
                        onOverride={actions.override}
                        catalog={catalog}
                        dayStartMinutes={statusInfo?.day_start_minutes ?? 0}
                        strictMode={statusInfo?.strict_mode ?? false}
                      />
                    )
                  }
                  more={
                    <div className="tt-today-more">
                      {pastDayEmpty ? (
                        <section className="tt-card">
                          <h2 className="tt-card-title">{t("usage.mostUsed")}</h2>
                          <p className="tt-sub">{t("hero.noUsageDay")}</p>
                        </section>
                      ) : (
                        <UsageAside
                          entries={apps}
                          total={total}
                          catalog={catalog}
                          onCategorize={actions.openCategorize}
                          onOpenAppDirectory={() => setAppDirectoryOpen(true)}
                        />
                      )}
                      <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                    </div>
                  }
                />
              </div>
            )}

            {activeTab === "limits" && (
              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <LimitsPage
                  catalog={catalog}
                  summary={summary}
                  statusInfo={statusInfo}
                  actions={actions}
                  notify={push}
                  onSetSetting={handleUpdateSetting}
                  settingPending={(key: string) => pendingSettings[key] ?? false}
                />
              </div>
            )}

            {activeTab === "settings" && (
              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <SettingsPage
                  statusInfo={statusInfo}
                  catalog={catalog}
                  actions={actions}
                  onSetSetting={handleUpdateSetting}
                  settingPending={(key: string) => pendingSettings[key] ?? false}
                  onOpenAppDirectory={() => setAppDirectoryOpen(true)}
                  theme={theme}
                  setTheme={setTheme}
                />
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
                cooldownHours={statusInfo?.limit_cooldown_hours ?? 24}
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
