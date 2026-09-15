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
    setAllowlist: (subjectType: string, subjectId: number, allowed: boolean) => Promise<void>;
}

export function useDowntime(notify?: (kind: "success" | "error", msg: string) => void): UseDowntime {
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
            if (notify) notify("success", "Schedule created successfully.");
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
            await apiUpdateSchedule(id, name, weekdayMask, startMinute, endMinute);
            if (notify) notify("success", "Schedule updated.");
            await refresh();
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh]);

    const toggleSchedule = useCallback(async (id: number, enabled: boolean) => {
        try {
            setSchedules((prev) =>
                prev.map((s) => (s.id === id ? { ...s, enabled } : s)),
            );
            await apiSetScheduleEnabled(id, enabled);
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            await refresh();
            throw e;
        }
    }, [notify, refresh]);

    const deleteSchedule = useCallback(async (id: number) => {
        try {
            await apiDeleteSchedule(id);
            if (notify) notify("success", "Schedule removed.");
            await refresh();
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh]);

    const setAllowlist = useCallback(async (
        subjectType: string,
        subjectId: number,
        allowed: boolean,
    ) => {
        try {
            await apiSetAllowlist(subjectType, subjectId, allowed);
            await refresh();
        } catch (e) {
            const err = describeError(e);
            if (notify) notify("error", err);
            throw e;
        }
    }, [notify, refresh]);

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
