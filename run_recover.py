import re

with open('ui/src/App.tsx', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Add imports
content = content.replace(
    'import { MetricCards } from "./components/MetricCards";',
    'import { MetricCards } from "./components/MetricCards";\nimport { FocusWidget } from "./components/FocusWidget";\nimport { UsageAside } from "./components/UsageAside";\nimport { CategoryMix } from "./components/CategoryMix";'
)

target = r'''<div className="card" style={{ padding: "24px" }}>
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
                </div>'''

replacement = r'''                  <div className="overview-activity-grid">
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

                  <div className="overview-chart-grid">
                    <div className="card" style={{ padding: "24px" }}>
                      <h3 style={{ fontSize: "16px", fontWeight: "600", margin: "0 0 16px 0" }}>Weekly History</h3>
                      <WeeklyChart week={week} viewDay={viewDay} loading={weekLoading} onSelectDay={setViewDay} />
                    </div>
                    <CategoryMix categories={categories} total={total} />
                  </div>'''

content = content.replace(target, replacement)
content = content.replace('{activeTab === "limits" && (\n              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>', '{activeTab === "limits" && (\n              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>')
content = content.replace('{activeTab === "web-filtering" && (\n              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>', '{activeTab === "web-filtering" && (\n              <div className="tab-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>')
content = content.replace('{activeTab === "settings" && (\n              <div style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>', '{activeTab === "settings" && (\n              <div className="tab-page settings-page" style={{ animation: "fade-in-scale 0.3s cubic-bezier(0.2, 0, 0, 1)" }}>\n\n                <div className="settings-page-intro">\n                  <span className="section-kicker">Workspace</span>\n                  <h2>{t("settings.title")}</h2>\n                  <p>Shape how Screentime tracks, protects, and presents your day.</p>\n                </div>')
content = content.replace('<section className="card">', '<section className="card settings-card settings-card--language">')
content = content.replace('<section className="card" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.appearance")}</h2>', '<section className="card settings-card settings-card--appearance" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.appearance")}</h2>')
content = content.replace('<section className="card" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>Advanced Parameters</h2>', '<section className="card settings-card settings-card--advanced" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>Advanced Parameters</h2>')
content = content.replace('<section className="card" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.security")}</h2>', '<section className="card settings-card settings-card--security" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.security")}</h2>')
content = content.replace('<section className="card" style={{ marginTop: 16 }}>\n                  <header className="card-header" style={{ cursor: \'pointer\', display: \'flex\', alignItems: \'center\', justifyContent: \'space-between\' }} onClick={() => setTutorialOpen(!tutorialOpen)}>\n                    <h2>{t("tutorial.title")}</h2>', '<section className="card settings-card settings-card--tutorial" style={{ marginTop: 16 }}>\n                  <header className="card-header" style={{ cursor: \'pointer\', display: \'flex\', alignItems: \'center\', justifyContent: \'space-between\' }} onClick={() => setTutorialOpen(!tutorialOpen)}>\n                    <h2>{t("tutorial.title")}</h2>')
content = content.replace('<section className="card" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.about")}</h2>', '<section className="card settings-card settings-card--about" style={{ marginTop: 16 }}>\n                  <header className="card-header">\n                    <h2>{t("settings.about")}</h2>')

with open('ui/src/App.tsx', 'w', encoding='utf-8') as f:
    f.write(content)
