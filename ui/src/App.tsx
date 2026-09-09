import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Monitor, Target, PauseCircle, Shield } from "lucide-react";
import { applyLanguage } from "./i18n";
import { formatDayLabel, formatDuration } from "./format";
import { MetricCards } from "./components/MetricCards";

import { BlockedBanner } from "./components/BlockedBanner";
import { CategorizeDialog } from "./components/CategorizeDialog";
import { Hero } from "./components/Hero";
import { LedgerRule } from "./components/LedgerRule";
import { LedgerSection } from "./components/LedgerSection";
import { LimitEditorDialog } from "./components/LimitEditorDialog";
import { LimitsPanel } from "./components/LimitsPanel";
import { OnboardingSlider } from "./components/OnboardingSlider";
import { PinGate } from "./components/PinGate";
import { PinSetupDialog } from "./components/PinSetupDialog";
import { Toasts } from "./components/Toasts";
import { TutorialContent } from "./components/TutorialContent";
import { WeeklyChart } from "./components/WeeklyChart";
import { WebFilteringPanel } from "./components/WebFilteringPanel";
import { Sidebar, type TabKey } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { useDashboard } from "./hooks/useDashboard";
import { useLedgerActions } from "./hooks/useLedgerActions";
import { useNowMinute } from "./hooks/useNowMinute";
import { useToasts } from "./hooks/useToasts";
import type { LimitDto } from "./types/generated/LimitDto";

const FIRST_RUN_KEY = "screentime_first_run_completed";

export function App() {
  const { t, i18n } = useTranslation();
  const {
    phase,
    statusInfo,
    summary,
    catalog,
    lastError,
    refreshCatalog,
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
  const { toasts, push, dismiss } = useToasts();
  const actions = useLedgerActions({
    catalog,
    pinConfigured: statusInfo?.pin_configured ?? false,
    notify: push,
    invalidate: refreshCatalog,
  });

  const [activeTab, setActiveTab] = useState<TabKey>("overview");

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
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)", display: "flex", flexDirection: "column", gap: "24px" }}>
                <MetricCards
                  metrics={[
                      {
                          label: "SCREEN TIME TODAY",
                          value: formatDuration(total),
                          icon: <Monitor size={20} color="var(--color-primary)" />,
                          trend: "Active screen time recorded"
                      },
                      {
                          label: "FOCUS TIME",
                          value: "3h 15m",
                          icon: <Target size={20} color="var(--color-primary)" />,
                          trend: "Placeholder (coming soon)",
                          badge: "57% of screen time"
                      },
                      {
                          label: "APP LIMITS HIT",
                          value: `${catalog?.limits.filter(l => l.target.kind === "app" && (summary?.apps ?? []).find(r => r.id === (l.target as any).id)?.blocked).length || 0} Apps`,
                          icon: <PauseCircle size={20} color="var(--color-danger)" />,
                          trend: "Suspended automatically",
                          badge: "Protected"
                      },
                      {
                          label: "DISTRACTIONS DEFLECTED",
                          value: "14 Sites",
                          icon: <Shield size={20} color="var(--color-primary)" />,
                          trend: "Estimated time saved: 45 mins"
                      }
                  ]}
                />
                
                <div className="card" style={{ padding: "24px" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "24px" }}>
                    <h3 style={{ fontSize: "16px", fontWeight: "600", margin: "0" }}>Daily Timeline</h3>
                    <div style={{ display: "flex", gap: "12px", alignItems: "center" }}>
                        <div className="date-picker-placeholder" style={{ display: "flex", alignItems: "center", background: "var(--bg-card)", border: "1px solid var(--border-subtle)", borderRadius: "8px", padding: "4px" }}>
                            <button className="btn btn-ghost btn-sm" onClick={goPrevDay}>&lt;</button>
                            <span style={{ padding: "0 12px", fontSize: "14px", fontWeight: "500" }}>{isViewingToday ? "Today" : formatDayLabel(viewDay!)}</span>
                            <button className="btn btn-ghost btn-sm" onClick={goNextDay} disabled={isViewingToday}>&gt;</button>
                        </div>
                    </div>
                  </div>
                  {pastDayEmpty ? (
                    <p className="empty-day">{t("hero.noUsageDay")}</p>
                  ) : (
                    <LedgerRule summary={summary} loading={loading} now={now} />
                  )}
                </div>

                {!pastDayEmpty && (
                  <div style={{ display: "flex", flexDirection: "column", gap: "24px" }}>
                    <LedgerSection
                      label={t("ledger.byApp", "By application")}
                      kind="app"
                      entries={apps}
                      total={total}
                      limitFor={limitFor}
                      canLimit={() => true}
                      busy={actions.busy}
                      onEdit={actions.openEditor}
                      onCategorize={actions.openCategorize}
                      catalog={catalog}
                    />
                    <LedgerSection
                      label={t("ledger.byCategory", "By category")}
                      kind="category"
                      entries={categories}
                      total={total}
                      limitFor={limitFor}
                      canLimit={(entry) => catalog?.categories.find((c) => c.id === entry.id)?.kind === "limitable"}
                      busy={actions.busy}
                      onEdit={actions.openEditor}
                    />
                  </div>
                )}
                
                <div className="card" style={{ padding: "24px" }}>
                  <h3 style={{ fontSize: "16px", fontWeight: "600", margin: "0 0 16px 0" }}>Weekly History</h3>
                  <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                </div>
              </div>
            )}

            {activeTab === "limits" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
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
                />
              </div>
            )}

            {activeTab === "web-filtering" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <WebFilteringPanel onAttempt={actions.attempt} />
              </div>
            )}

            {activeTab === "settings" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>

                {/* Language */}
                <section className="card">
                  <header className="card-header">
                    <h2>{t("settings.language")}</h2>
                    <div className="card-subtitle">{t("settings.languageDesc")}</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">{t("settings.language")}</div>
                      </div>
                      <select
                        className="select-input"
                        value={i18n.language}
                        onChange={(e) => {
                          applyLanguage(e.target.value);
                        }}
                      >
                        <option value="en">{t("settings.english")}</option>
                        <option value="ar">{t("settings.arabic")}</option>
                      </select>
                    </div>
                  </div>
                </section>

                {/* Appearance */}
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>{t("settings.appearance")}</h2>
                    <div className="card-subtitle">{t("settings.appearanceDesc")}</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">{t("settings.showHud")}</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          {t("settings.showHudDesc")}
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={statusInfo?.show_hud_overlay ?? true}
                          onChange={(e) => {
                            actions.setSetting("show_hud_overlay", e.target.checked.toString());
                          }}
                        />
                        <span className="slider"></span>
                      </label>
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 16 }}>
                      <div>
                        <div className="form-label">Social media protection</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Blocks a small curated list of social-media domains. Adult filtering remains handled by Family DNS.
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={statusInfo?.social_filter_enabled ?? false}
                          onChange={(e) => {
                            actions.setSetting("social_filter_enabled", e.target.checked.toString());
                          }}
                        />
                        <span className="slider"></span>
                      </label>
                    </div>
                  </div>
                </section>

                {/* Advanced Parameters */}
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>Advanced Parameters</h2>
                    <div className="card-subtitle">Fine-tune system thresholds and enforcement behaviour.</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Anti-impulse Cooldown (Hours)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Time delay before a relaxed limit takes effect. Tightening applies instantly.
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        key={`cooldown_${statusInfo?.limit_cooldown_hours}`}
                        defaultValue={statusInfo?.limit_cooldown_hours?.toString() ?? "24"}
                        onBlur={(e) => actions.setSetting("limit_cooldown_hours", e.target.value)}
                      />
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Idle Threshold (Seconds)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Seconds without input before usage stops accruing.
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        key={`idle_${statusInfo?.idle_threshold_secs}`}
                        defaultValue={statusInfo?.idle_threshold_secs?.toString() ?? "60"}
                        onBlur={(e) => actions.setSetting("idle_threshold_secs", e.target.value)}
                      />
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">Day Start Offset (Minutes)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Minutes after local midnight at which daily budgets reset (0 = midnight).
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        key={`day_start_${statusInfo?.day_start_minutes}`}
                        defaultValue={statusInfo?.day_start_minutes?.toString() ?? "0"}
                        onBlur={(e) => actions.setSetting("day_start_minutes", e.target.value)}
                      />
                    </div>
                  </div>
                </section>

                {/* Security */}
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>{t("settings.security")}</h2>
                    <div className="card-subtitle">{t("settings.securityDesc")}</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Strict Mode</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Prevents circumvention by blocking task manager and registry edits while limits are active.
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={statusInfo?.strict_mode ?? false}
                          onChange={(e) => {
                            actions.setSetting("strict_mode", e.target.checked.toString());
                          }}
                        />
                        <span className="slider"></span>
                      </label>
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
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
                  </div>
                </section>

                {/* How to Use (collapsible) */}
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header" style={{ cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }} onClick={() => setTutorialOpen(!tutorialOpen)}>
                    <h2>{t("tutorial.title")}</h2>
                    <span>{tutorialOpen ? "▲" : "▼"}</span>
                  </header>
                  <div style={{ maxHeight: tutorialOpen ? '2000px' : '0', opacity: tutorialOpen ? 1 : 0, overflow: 'hidden', transition: 'all 0.4s ease-in-out' }}>
                    <div className="card-body">
                      <TutorialContent />
                    </div>
                  </div>
                </section>

                {/* About */}
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>{t("settings.about")}</h2>
                  </header>
                  <div className="card-body">
                    <p className="form-hint">{t("settings.aboutVersion")}</p>
                  </div>
                </section>
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
