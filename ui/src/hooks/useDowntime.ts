import { useCallback, useEffect, useState } from "react";
import {
    listSchedules,
    createSchedule as apiCreateSchedule,
    updateSchedule as apiUpdateSchedule,
    setScheduleEnabled as apiSetScheduleEnabled,
    deleteSchedule as apiDeleteSchedule,
    listAllowlist as apiListAllowlist,
    setAllowlist as apiSetAllowlist,
    describeError,
} from "../api";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import type { AllowlistItemDto } from "../types/generated/AllowlistItemDto";
import type { Guarded } from "./useLedgerActions";
import i18n from "../i18n";

/** Fallback when no PIN gate is wired in: just run the action without a PIN. */
const unguarded: Guarded = (_label, op) => op(undefined);

export interface UseDowntime {
    schedules: ScheduleDto[];
    allowlist: AllowlistItemDto[];
    loading: boolean;
    error: string | null;
    refresh: () => Promise<void>;
    createSchedule: (name: string, weekdayMask: number, startMinute: number, endMinute: number) => Promise<void>;
    updateSchedule: (id: number, name: string, weekdayMask: number, startMinute: number, endMinute: number) => Promise<void>;
    toggleSchedule: (id: number, enabled: boolean) => Promise<void>;
    deleteSchedule: (id: number) => Promise<void>;
    /** `label` names the subject in the PIN prompt when allowlisting needs one. */
    setAllowlist: (subjectType: string, subjectId: number, allowed: boolean, label?: string) => Promise<void>;
}

export function useDowntime(
    notify?: (kind: "success" | "error", msg: string) => void,
    guarded: Guarded = unguarded,
): UseDowntime {
    const [schedules, setSchedules] = useState<ScheduleDto[]>([]);
    const [allowlist, setAllowlistItems] = useState<AllowlistItemDto[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        try {
            setLoading(true);
            const [schedsRes, allowRes] = await Promise.all([
                listSchedules(),
                apiListAllowlist(),
            ]);
            setSchedules(schedsRes.schedules);
            setAllowlistItems(allowRes.items);
            setError(null);
        } catch (e) {
            const err = describeError(e);
            setError(err);
            if (notify) notify("error", err);
        } finally {
            setLoading(false);
        }
    }, [notify]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    const createSchedule = useCallback(async (
        name: string,
        weekdayMask: number,
        startMinute: number,
        endMinute: number,
    ) => {
        try {
            await apiCreateSchedule(name, weekdayMask, startMinute, endMinute);
            if (notify) notify("success", i18n.t("downtime.created"));
            await refresh();
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh]);

    const updateSchedule = useCallback(async (
        id: number,
        name: string,
        weekdayMask: number,
        startMinute: number,
        endMinute: number,
    ) => {
        try {
            await guarded(i18n.t("pinGate.editSchedule", { name }), async (pin) => {
                await apiUpdateSchedule(id, name, weekdayMask, startMinute, endMinute, pin);
                if (notify) notify("success", i18n.t("downtime.updated"));
                await refresh();
            });
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh, guarded]);

    const toggleSchedule = useCallback(async (id: number, enabled: boolean) => {
        const name = schedules.find((s) => s.id === id)?.name ?? "";
        try {
            // Disabling may need the PIN; state only flips once the agent has
            // accepted, so a cancelled prompt leaves the switch truthful.
            await guarded(i18n.t("pinGate.disableSchedule", { name }), async (pin) => {
                await apiSetScheduleEnabled(id, enabled, pin);
                setSchedules((prev) =>
                    prev.map((s) => (s.id === id ? { ...s, enabled } : s)),
                );
            });
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            await refresh();
            throw e;
        }
    }, [notify, refresh, guarded, schedules]);

    const deleteSchedule = useCallback(async (id: number) => {
        const name = schedules.find((s) => s.id === id)?.name ?? "";
        try {
            await guarded(i18n.t("pinGate.deleteSchedule", { name }), async (pin) => {
                await apiDeleteSchedule(id, pin);
                if (notify) notify("success", i18n.t("downtime.removed"));
                await refresh();
            });
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh, guarded, schedules]);

    const setAllowlist = useCallback(async (
        subjectType: string,
        subjectId: number,
        allowed: boolean,
        label?: string,
    ) => {
        try {
            await guarded(i18n.t("pinGate.allowApp", { app: label ?? `#${subjectId}` }), async (pin) => {
                await apiSetAllowlist(subjectType, subjectId, allowed, pin);
                await refresh();
            });
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh, guarded]);

    return {
        schedules,
        allowlist,
        loading,
        error,
        refresh,
        createSchedule,
        updateSchedule,
        toggleSchedule,
        deleteSchedule,
        setAllowlist,
    };
}
