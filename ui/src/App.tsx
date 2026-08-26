import { BlockedBanner } from "./components/BlockedBanner";
import { Hero } from "./components/Hero";
import { LedgerRule } from "./components/LedgerRule";
import { LedgerSection } from "./components/LedgerSection";
import { LimitEditorDialog } from "./components/LimitEditorDialog";
import { LimitsPanel } from "./components/LimitsPanel";
import { Masthead } from "./components/Masthead";
import { PinGate } from "./components/PinGate";
import { PinSetupDialog } from "./components/PinSetupDialog";
import { Toasts } from "./components/Toasts";
import { useDashboard } from "./hooks/useDashboard";
import { useLedgerActions } from "./hooks/useLedgerActions";
import { useNowMinute } from "./hooks/useNowMinute";
import { useToasts } from "./hooks/useToasts";
import { useWindowChrome } from "./hooks/useWindowChrome";
import type { LimitDto } from "./types/generated/LimitDto";

export function App() {
    const { phase, statusInfo, summary, catalog, lastError, refreshCatalog } = useDashboard();
    const now = useNowMinute();
    const { toasts, push, dismiss } = useToasts();
    const actions = useLedgerActions({
        catalog,
        pinConfigured: statusInfo?.pin_configured ?? false,
        notify: push,
        invalidate: refreshCatalog,
    });
    const { isMaximized } = useWindowChrome();

    const loading = phase === "connecting" && summary === null;
    const total = summary?.total_seconds ?? 0;
    const categories = (summary?.categories ?? []).filter((row) => row.seconds > 0);
    const apps = (summary?.apps ?? []).filter((row) => row.seconds > 0);
    const blocked = (summary?.apps ?? []).filter((row) => row.blocked);

    function limitFor(kind: "app" | "category", id: number): LimitDto | undefined {
        return catalog?.limits.find((limit) => limit.target.kind === kind && limit.target.id === id);
    }

    return (
        <main className={`sheet${isMaximized ? " maximized-inset" : ""}`}>
            <Toasts toasts={toasts} dismiss={dismiss} />
            <Masthead phase={phase} now={now} />
            <div className="rule-double" />

            {phase === "live" && statusInfo !== null && !statusInfo.tracking_available && (
                <p className="notice">
                    Tracking unavailable — the tracker cannot see focused windows on this system.
                </p>
            )}
            {phase === "offline" && lastError !== null && (
                <p className="notice notice--dim">Retrying the ledger · {lastError}</p>
            )}

            <Hero summary={summary} loading={loading} />
            <LedgerRule summary={summary} loading={loading} now={now} />

            <BlockedBanner blocked={blocked} busy={actions.busy} onOverride={actions.override} />

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
            />

            <LimitsPanel
                limits={catalog?.limits ?? []}
                catalog={catalog}
                busy={actions.busy}
                pinConfigured={statusInfo?.pin_configured ?? false}
                onToggle={actions.toggleLimit}
                onEdit={actions.openEditor}
                onRemove={actions.removeLimit}
                onNew={actions.startNewOrder}
                onOpenPinSetup={actions.openPinSetup}
            />

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
        </main>
    );
}
