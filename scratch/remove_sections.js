
const fs = require("fs");
let app = fs.readFileSync("ui/src/App.tsx", "utf8");

const target = `                  {!pastDayEmpty && (
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
                  )}`;
app = app.replace(target, "");
// also remove double blank lines
app = app.replace(/\n\s*\n\s*\n/g, "\n\n");
fs.writeFileSync("ui/src/App.tsx", app);

