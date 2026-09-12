import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Clock, Flame, Hourglass, ShieldCheck, ChevronDown } from "lucide-react";
import { applyLanguage } from "./i18n";
import { formatDayLabel, formatDuration } from "./format";
import { MetricCards } from "./components/MetricCards";
import { UsageAside } from "./components/UsageAside";
import { CategoryMix } from "./components/CategoryMix";

import { BlockedBanner } from "./components/BlockedBanner";
import { CategorizeDialog } from "./components/CategorizeDialog";
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
  const { toasts, push, dismiss } = useToasts();
  const actions = useLedgerActions({
    catalog,
    pinConfigured: statusInfo?.pin_configured ?? false,
    notify: push,
    invalidate: refreshCatalog,
    refreshStatus,
  });

  const [activeTab, setActiveTab] = useState<TabKey>("overview");
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
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)", display: "flex", flexDirection: "column", gap: "24px" }}>
                <MetricCards
                  metrics={[
                    {
                      label: "SCREEN TIME TODAY",
                      value: formatDuration(total),
                      icon: <Clock size={18} color="var(--color-primary)" />,
                      trend: vsAvgText,
                      badge: total > 0 ? `${apps.length} apps` : undefined,
                    },
                    {
                      label: "LONGEST FOCUS STREAK",
                      value: maxIntervalSeconds > 0 ? formatDuration(maxIntervalSeconds) : "—",
                      icon: <Flame size={18} color="#f59e0b" />,
                      trend: streakSubtitle,
                      badge: streakBadge,
                    },
                    {
                      label: "REMAINING BUDGET",
                      value: enabledLimits.length > 0 ? formatDuration(remainingBudgetSeconds) : "No limits",
                      icon: <Hourglass size={18} color="var(--color-primary)" />,
                      trend: enabledLimits.length > 0
                        ? `${enabledLimits.length} active budget${enabledLimits.length === 1 ? "" : "s"}`
                        : "Configure in App Limits",
                      badge: enabledLimits.length > 0
                        ? remainingBudgetSeconds === 0
                          ? "Exhausted"
                          : "In Budget"
                        : undefined,
                    },
                    {
                      label: "PROTECTION STATUS",
                      value: blockedCount > 0 ? `${blockedCount} Blocked` : "Active",
                      icon: (
                        <ShieldCheck
                          size={18}
                          color={blockedCount > 0 ? "var(--color-danger)" : "var(--color-success)"}
                        />
                      ),
                      trend: blockedCount > 0
                        ? `${blocked.map((a) => a.label).join(", ")} suspended`
                        : "Zero limit violations today",
                      badge: blockedCount > 0 ? "Action Needed" : "Protected",
                    },
                  ]}
                />
                
                <div className="overview-activity-grid">
                  <div className="card timeline-panel">
                    <div className="timeline-panel__header">
                      <div>
                        <p className="panel-eyebrow">Today at a glance</p>
                        <h3>Daily Timeline</h3>
                      </div>
                      <div className="date-picker-placeholder">
                        <button className="btn btn-ghost btn-sm" onClick={goPrevDay}>&lt;</button>
                        <span>{isViewingToday ? "Today" : formatDayLabel(viewDay!)}</span>
                        <button className="btn btn-ghost btn-sm" onClick={goNextDay} disabled={isViewingToday}>&gt;</button>
                      </div>
                    </div>
                    {pastDayEmpty ? (
                      <p className="empty-day">{t("hero.noUsageDay")}</p>
                    ) : (
                      <LedgerRule summary={summary} loading={loading} now={now} catalog={catalog} />
                    )}
                  </div>
                  <UsageAside entries={apps} total={total} />
                </div>

                  <div className="overview-chart-grid">
                    <div className="card" style={{ padding: "24px" }}>
                      <h3 style={{ fontSize: "16px", fontWeight: "600", margin: "0 0 16px 0" }}>Weekly History</h3>
                      <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                    </div>
                    <CategoryMix categories={categories} total={total} />
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

                {/* Language */}
                <section className="card settings-card settings-card--language">
                  <header className="card-header">
                    <h2>{t("settings.language")}</h2>
                    <div className="card-subtitle">{t("settings.languageDesc")}</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">{t("settings.language")}</div>
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
                  </div>
                </section>

                {/* Appearance */}
                <section className="card settings-card settings-card--appearance" style={{ marginTop: 16 }}>
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
                  </div>
                </section>

                {/* Advanced Parameters */}
                <section className="card settings-card settings-card--advanced" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>Advanced Parameters</h2>
                    <div className="card-subtitle">Fine-tune system thresholds and enforcement behaviour.</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Anti-impulse Cooldown</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Time delay before a relaxed limit takes effect. Tightening applies instantly.
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
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Idle Threshold</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Seconds without input before usage stops accruing.
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
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">Day Start Offset</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Minutes after local midnight at which daily budgets reset (0 = midnight).
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

                {/* Security */}
                <section className="card settings-card settings-card--security" style={{ marginTop: 16 }}>
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

                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
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
                <section className="card settings-card settings-card--tutorial" style={{ marginTop: 16 }}>
                  <header className="card-header" style={{ cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }} onClick={() => setTutorialOpen(!tutorialOpen)}>
                    <h2>{t("tutorial.title")}</h2>
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
                    <div className="card-body">
                      <TutorialContent />
                    </div>
                  </div>
                </section>

                {/* About */}
                <section className="card settings-card settings-card--about" style={{ marginTop: 16 }}>
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
