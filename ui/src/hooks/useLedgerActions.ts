import { useCallback, useRef, useState } from "react";

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
        (target: LimitTargetDto, minutes: number, enabled: boolean) => {
            if (busyRef.current) return;
            attempt(`Apply the order for ${targetLabel(target, depsRef.current.catalog)}`, async (pin) => {
                const effective = await api.setLimit(target, minutes, enabled, pin);
                depsRef.current.notify("success", `Order recorded.${effectClause(effective)}`);
                depsRef.current.invalidate();
                closeEditor();
            });
        },
        [attempt, closeEditor],
    );

    const toggleLimit = useCallback(
        (limit: LimitDto, next: boolean) => {
            attempt(
                `${next ? "Reinstate" : "Suspend"} the order for ${targetLabel(limit.target, depsRef.current.catalog)}`,
                async (pin) => {
                    const effective = await api.setLimit(limit.target, limit.default_minutes, next, pin);
                    depsRef.current.notify(
                        next ? "success" : "info",
                        `Order ${next ? "reinstated" : "suspended"}.${effectClause(effective)}`,
                    );
                    depsRef.current.invalidate();
                },
            );
        },
        [attempt],
    );

    const removeLimit = useCallback(
        (target: LimitTargetDto) => {
            attempt(`Remove the order for ${targetLabel(target, depsRef.current.catalog)}`, async (pin) => {
                const effective = await api.deleteLimit(target, pin);
                depsRef.current.notify("info", `Order removed.${effectClause(effective)}`);
                depsRef.current.invalidate();
            });
        },
        [attempt],
    );

    const override = useCallback(
        (row: UsageRowDto) => {
            attempt(`Grant fifteen more minutes to ${row.label}`, async (pin) => {
                await api.grantOverride({ kind: "app", id: row.id }, api.OVERRIDE_SECONDS, pin);
                depsRef.current.notify("success", `+15 minutes granted to ${row.label}.`);
            });
        },
        [attempt],
    );

    const openPinSetup = useCallback(() => setPinSetupOpen(true), []);
    const closePinSetup = useCallback(() => setPinSetupOpen(false), []);

    const submitPinSetup = useCallback(async (newPin: string, currentPin: string) => {
        await api.setPin(newPin, currentPin.length > 0 ? currentPin : null);
        setPinSetupOpen(false);
        depsRef.current.notify("success", "PIN saved.");
    }, []);

    return {
        busy,
        editor,
        gate,
        gateError,
        pinSetupOpen,
        openEditor,
        startNewOrder,
        closeEditor,
        submitEditor,
        toggleLimit,
        removeLimit,
        override,
        submitGate,
        cancelGate,
        openPinSetup,
        closePinSetup,
        submitPinSetup,
    };
}
