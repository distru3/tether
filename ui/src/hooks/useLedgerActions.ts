import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import * as api from "../api";
import { effectClause, targetLabel } from "../format";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { ToastKind } from "./useToasts";

interface Deps {
    catalog: CatalogDto | null;
    pinConfigured: boolean;
    notify: (kind: ToastKind, message: string) => void;
    invalidate: () => void;
    refreshStatus?: () => Promise<void>;
}

export interface GateRequest {
    label: string;
    run: (pin: string) => Promise<void>;
}

export interface EditorRequest {
    target: LimitTargetDto | null;
    limit: LimitDto | null;
}

export function useLedgerActions(deps: Deps) {
    const { t } = useTranslation();
    const [busy, setBusy] = useState(false);

    const [editor, setEditor] = useState<EditorRequest | null>(null);
    const [gate, setGate] = useState<GateRequest | null>(null);
    const [gateError, setGateError] = useState<string | null>(null);
    const [pinSetupOpen, setPinSetupOpen] = useState(false);

    const busyRef = useRef(false);
    const gateRef = useRef<GateRequest | null>(null);
    const editorRef = useRef<EditorRequest | null>(null);
    const depsRef = useRef<Deps>(deps);
    depsRef.current = deps;

    const routeFailure = useCallback((error: unknown) => {
        const message = api.describeError(error);
        if (gateRef.current !== null) setGateError(message);
        else depsRef.current.notify("error", message);
    }, []);

    const runExclusive = useCallback(
        (op: () => Promise<void>) => {
            if (busyRef.current) return;
            busyRef.current = true;
            setBusy(true);
            void op()
                .catch(routeFailure)
                .finally(() => {
                    busyRef.current = false;
                    setBusy(false);
                });
        },
        [routeFailure],
    );

    const attempt = useCallback(
        (label: string, makeOp: (pin: string) => Promise<void>) => {
            if (depsRef.current.pinConfigured) {
                setGateError(null);
                const request = { label, run: makeOp };
                gateRef.current = request;
                setGate(request);
            } else {
                runExclusive(() => makeOp(""));
            }
        },
        [runExclusive],
    );

    const submitGate = useCallback(
        (pin: string) => {
            const request = gateRef.current;
            if (!request || busyRef.current) return;
            runExclusive(async () => {
                await request.run(pin);
                gateRef.current = null;
                setGate(null);
                setGateError(null);
            });
        },
        [runExclusive],
    );

    const cancelGate = useCallback(() => {
        if (busyRef.current) return;
        gateRef.current = null;
        setGate(null);
        setGateError(null);
    }, []);

    const openEditor = useCallback((target: LimitTargetDto | null, limit: LimitDto | null) => {
        const next = { target, limit };
        editorRef.current = next;
        setEditor(next);
    }, []);

    const startNewOrder = useCallback(() => {
        // Null target: the editor shows its picker (total / category / app).
        openEditor(null, null);
    }, [openEditor]);

    const closeEditor = useCallback(() => {
        editorRef.current = null;
        setEditor(null);
    }, []);

    const submitEditor = useCallback(
        (target: LimitTargetDto, minutes: number, weekdayMinutes: api.WeekdayMinutes, enabled: boolean) => {
            if (busyRef.current) return;
            attempt(t("pinGate.applyOrder", { target: targetLabel(target, depsRef.current.catalog) }), async (pin) => {
                const effective = await api.setLimit(target, minutes, weekdayMinutes, enabled, pin);
                depsRef.current.notify("success", t("actions.orderRecorded") + effectClause(effective));
                depsRef.current.invalidate();
                closeEditor();
            });
        },
        [attempt, closeEditor],
    );

    const toggleLimit = useCallback(
        (limit: LimitDto, next: boolean) => {
            attempt(
                next
                    ? t("pinGate.reinstateOrder", { target: targetLabel(limit.target, depsRef.current.catalog) })
                    : t("pinGate.suspendOrder", { target: targetLabel(limit.target, depsRef.current.catalog) }),
                async (pin) => {
                    // Toggling is a re-issue of the same standing order: the
                    // per-day overrides travel through untouched.
                    const effective = await api.setLimit(
                        limit.target,
                        limit.default_minutes,
                        limit.weekday_minutes,
                        next,
                        pin,
                    );
                    depsRef.current.notify(next ? "success" : "info", (next ? t("actions.orderReinstated") : t("actions.orderSuspended")) + effectClause(effective));
                    depsRef.current.invalidate();
                },
            );
        },
        [attempt, t],
    );

    const removeLimit = useCallback(
        (target: LimitTargetDto) => {
            attempt(t("pinGate.removeOrder", { target: targetLabel(target, depsRef.current.catalog) }), async (pin) => {
                const effective = await api.deleteLimit(target, pin);
                depsRef.current.notify("info", t("actions.orderRemoved") + effectClause(effective));
                depsRef.current.invalidate();
            });
        },
        [attempt],
    );

    const cancelPendingLimit = useCallback(
        (target: LimitTargetDto) => {
            attempt(t("pinGate.cancelPending", { target: targetLabel(target, depsRef.current.catalog) }), async (pin) => {
                const effective = await api.cancelPendingLimit(target, pin);
                depsRef.current.notify("info", t("actions.pendingCancelled"));
                depsRef.current.invalidate();
            });
        },
        [attempt],
    );

    const override = useCallback(
        (row: UsageRowDto) => {
            attempt(t("pinGate.grantMinutes", { target: row.label }), async (pin) => {
                await api.grantOverride({ kind: "app", id: row.id }, api.OVERRIDE_SECONDS, pin);
                depsRef.current.notify("success", t("actions.minutesGranted", { app: row.label }));
            });
        },
        [attempt],
    );

    const openPinSetup = useCallback(() => setPinSetupOpen(true), []);
    const closePinSetup = useCallback(() => setPinSetupOpen(false), []);

    const submitPinSetup = useCallback(async (newPin: string, currentPin: string) => {
        const reply = await api.setPin(newPin, currentPin.length > 0 ? currentPin : null);
        depsRef.current.notify("success", t("actions.pinSaved"));
        // The dialog stays open to display the one-time recovery code; it
        // closes when the user confirms they wrote the code down.
        return reply;
    }, []);

    const [categorizeTarget, setCategorizeTarget] = useState<{
        appId: number;
        appName: string;
        primaryId: number | null;
        tagIds: number[];
    } | null>(null);

    const openCategorize = useCallback(
        (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => {
            setCategorizeTarget({ appId, appName, primaryId, tagIds });
        },
        [],
    );

    const closeCategorize = useCallback(() => setCategorizeTarget(null), []);

    const submitCategorize = useCallback(
        (primaryId: number, tagIds: number[]) => {
            if (categorizeTarget === null) return;
            runExclusive(async () => {
                await api.categorizeApp(categorizeTarget.appId, primaryId, tagIds);
                depsRef.current.notify("success", t("actions.categorized", { app: categorizeTarget.appName }));
                depsRef.current.invalidate();
                setCategorizeTarget(null);
            });
        },
        [categorizeTarget, runExclusive],
    );

    const resetCategorize = useCallback(() => {
        if (categorizeTarget === null) return;
        runExclusive(async () => {
            await api.categorizeApp(categorizeTarget.appId, null, []);
            depsRef.current.notify("info", t("actions.resetAutoDetect", { app: categorizeTarget.appName }));
            depsRef.current.invalidate();
            setCategorizeTarget(null);
        });
    }, [categorizeTarget, runExclusive]);

    const setSetting = useCallback(async (key: string, value: string) => {
        try {
            setBusy(true);
            await api.setSetting(key, value);
            if (deps.refreshStatus) {
                await deps.refreshStatus();
            }
            deps.invalidate();
            deps.notify("success", t("actions.settingSaved"));
        } catch (e) {
            deps.notify("error", String(e));
            throw e;
        } finally {
            setBusy(false);
        }
    }, [deps, t]);

    return {
        attempt,
        busy,
        setSetting,
        editor,
        gate,
        gateError,
        pinSetupOpen,
        categorizeTarget,
        openEditor,
        startNewOrder,
        closeEditor,
        submitEditor,
        toggleLimit,
        removeLimit,
        cancelPendingLimit,
        override,
        submitGate,
        cancelGate,
        openPinSetup,
        closePinSetup,
        submitPinSetup,
        openCategorize,
        closeCategorize,
        submitCategorize,
        resetCategorize,
    };
}
