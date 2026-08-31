import { useEffect, useState } from "react";

import { BlockedBanner } from "./components/BlockedBanner";
import { CategorizeDialog } from "./components/CategorizeDialog";
import { Hero } from "./components/Hero";
import { LedgerRule } from "./components/LedgerRule";
import { LedgerSection } from "./components/LedgerSection";
import { LimitEditorDialog } from "./components/LimitEditorDialog";
import { LimitsPanel } from "./components/LimitsPanel";
import { PinGate } from "./components/PinGate";
import { PinSetupDialog } from "./components/PinSetupDialog";
import { Toasts } from "./components/Toasts";
import { WeeklyChart } from "./components/WeeklyChart";
import { WebFilteringPanel } from "./components/WebFilteringPanel";
import { Sidebar, type TabKey } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { useDashboard } from "./hooks/useDashboard";
import { useLedgerActions } from "./hooks/useLedgerActions";
import { useNowMinute } from "./hooks/useNowMinute";
import { useToasts } from "./hooks/useToasts";
import type { LimitDto } from "./types/generated/LimitDto";

export function App() {
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

  const activeLimitsCount = catalog?.limits.length ?? 0;

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
                Tracking unavailable - the tracker cannot see focused windows on this system.
              </p>
            )}
            {phase === "offline" && lastError !== null && (
              <p className="notice notice--dim">Retrying the ledger - {lastError}</p>
            )}

            {activeTab === "overview" && (
              <div className="overview-grid" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <div className="overview-left-col">
                  <Hero
                    summary={summary}
                    loading={loading}
                    viewDay={viewDay}
                    isViewingToday={isViewingToday}
                    goPrevDay={goPrevDay}
                    goNextDay={goNextDay}
                    goToday={goToday}
                  />
                  {pastDayEmpty ? (
                    <p className="empty-day">No usage recorded for this day.</p>
                  ) : (
                    <LedgerRule summary={summary} loading={loading} now={now} />
                  )}

                  <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                </div>

                <div className="overview-right-col">
                  <BlockedBanner blocked={blocked} busy={actions.busy} onOverride={actions.override} />

                  {!pastDayEmpty && (
                    <>
                      <LedgerSection
                        label="By category"
                        kind="category"
                        entries={categories}
                        total={total}
                        limitFor={limitFor}
                        canLimit={(entry) => catalog?.categories.find((c) => c.id === entry.id)?.kind === "limitable"}
                        busy={actions.busy}
                        onEdit={actions.openEditor}
                      />
                      <LedgerSection
                        label="By application"
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
                    </>
                  )}
                </div>
              </div>
            )}

            {activeTab === "limits" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>
                <LimitsPanel
                  limits={catalog?.limits ?? []}
                  catalog={catalog}
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
                <WebFilteringPanel />
              </div>
            )}

            {activeTab === "settings" && (
              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>

                <section className="card">
                  <header className="card-header">
                    <h2>Appearance & Experience</h2>
                    <div className="card-subtitle">Customize how limits are displayed during use.</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">Show Remaining Time HUD</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Displays a tiny overlay at the top of limited apps with the remaining time.
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
                  </div>
                </section>
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>Security</h2>

                    <div className="card-subtitle">Manage parental controls and protection.</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">Administrator PIN</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          {statusInfo?.pin_configured ? "A PIN is currently required to change limits." : "No PIN is set. Limits can be removed freely."}
                        </div>
                      </div>
                      <button
                        type="button"
                        className="btn btn-secondary"
                        onClick={actions.openPinSetup}
                      >
                        {statusInfo?.pin_configured ? "Change / Remove PIN" : "Set PIN"}
                      </button>
                    </div>
                  </div>
                </section>
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>About Screentime</h2>
                  </header>
                  <div className="card-body">
                    <p className="form-hint">Screentime Agent v0.1.0 (Live mode)</p>
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
