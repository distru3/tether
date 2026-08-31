import { useCallback, useEffect, useRef, useState } from "react";

import { describeError, getCatalog, getDaySummary, getStatus, getWeeklySummary } from "../api";
import { dayKeyToDate, shiftDay, todayKey } from "../format";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { StatusDto } from "../types/generated/StatusDto";
import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";

export type Phase = "connecting" | "live" | "offline";

// One poll per visible second: a local named-pipe round trip plus one small
// SQLite read is well under a millisecond on the agent side, and 1 Hz is what
// makes the ledger feel like it is counting rather than refreshing.
const POLL_MS = 1000;
const CATALOG_MS = 60000;
// The weekly trend is a seven-day aggregate scan, so it refreshes on a slower
// visibility-gated 30 s cadence, plus an immediate refetch whenever the
// browsed day changes (fetchWeek is keyed on viewDay).
const WEEK_MS = 30000;

export interface Dashboard {
    phase: Phase;
    statusInfo: StatusDto | null;
    summary: DaySummaryDto | null;
    catalog: CatalogDto | null;
    lastError: string | null;
    todayKey: number;
    viewDay: number;
    isViewingToday: boolean;
    week: WeeklySummaryDto | null;
    weekLoading: boolean;
    refreshCatalog: () => void;
    setViewDay: (day: number) => void;
    goPrevDay: () => void;
    goNextDay: () => void;
    goToday: () => void;
}

export function useDashboard(): Dashboard {
    const [phase, setPhase] = useState<Phase>("connecting");
    const [statusInfo, setStatusInfo] = useState<StatusDto | null>(null);
    const [summary, setSummary] = useState<DaySummaryDto | null>(null);
    const [catalog, setCatalog] = useState<CatalogDto | null>(null);
    const [lastError, setLastError] = useState<string | null>(null);
    const [dayKey, setDayKey] = useState<number>(() => todayKey());
    const [viewDay, setViewDayRaw] = useState<number>(() => todayKey());
    const [followingToday, setFollowingToday] = useState(true);
    const [week, setWeek] = useState<WeeklySummaryDto | null>(null);
    const [weekLoading, setWeekLoading] = useState(false);

    const pollSeq = useRef(0);
    const catalogSeq = useRef(0);
    const weekSeq = useRef(0);

    const isViewingToday = viewDay === todayKey();

    const poll = useCallback(async () => {
        const ticket = ++pollSeq.current;
        try {
            const status = await getStatus();
            if (ticket !== pollSeq.current) return;
            setStatusInfo(status);
            const nextSummary = await getDaySummary(viewDay);
            if (ticket !== pollSeq.current) return;
            setSummary(nextSummary);
            setPhase("live");
            setLastError(null);
        } catch (error) {
            if (ticket !== pollSeq.current) return;
            setPhase("offline");
            setLastError(describeError(error));
        }
    }, [viewDay]);

    const refreshCatalog = useCallback(() => {
        const ticket = ++catalogSeq.current;
        getCatalog()
            .then((next) => {
                if (ticket === catalogSeq.current) setCatalog(next);
            })
            .catch(() => { });
    }, []);

    const fetchWeek = useCallback(async () => {
        const ticket = ++weekSeq.current;
        setWeekLoading(true);
        try {
            const jsDay = dayKeyToDate(viewDay).getDay();
            const deltaToSunday = (7 - jsDay) % 7;
            const weekEndDay = shiftDay(viewDay, deltaToSunday);
            const next = await getWeeklySummary(weekEndDay);
            if (ticket !== weekSeq.current) return;
            setWeek(next);
        } catch {
            if (ticket !== weekSeq.current) return;
        } finally {
            if (ticket === weekSeq.current) setWeekLoading(false);
        }
    }, [viewDay]);

    const setViewDay = useCallback((day: number) => {
        const capped = Math.min(day, todayKey());
        setViewDayRaw(capped);
        setFollowingToday(capped === todayKey());
    }, []);

    const goPrevDay = useCallback(() => {
        setViewDay(shiftDay(viewDay, -1));
    }, [viewDay, setViewDay]);

    const goNextDay = useCallback(() => {
        const next = shiftDay(viewDay, 1);
        if (next > todayKey()) return;
        setViewDay(next);
    }, [viewDay, setViewDay]);

    const goToday = useCallback(() => {
        const today = todayKey();
        setViewDayRaw(today);
        setFollowingToday(true);
    }, []);

    useEffect(() => {
        void poll();
        refreshCatalog();
    }, [poll, refreshCatalog]);

    useEffect(() => {
        const timer = window.setInterval(() => {
            if (!document.hidden) void poll();
        }, POLL_MS);
        const onVisibility = () => {
            if (!document.hidden) {
                void poll();
                refreshCatalog();
            }
        };
        document.addEventListener("visibilitychange", onVisibility);
        return () => {
            window.clearInterval(timer);
            document.removeEventListener("visibilitychange", onVisibility);
        };
    }, [poll, refreshCatalog]);

    useEffect(() => {
        const timer = window.setInterval(() => {
            if (!document.hidden) refreshCatalog();
        }, CATALOG_MS);
        return () => window.clearInterval(timer);
    }, [refreshCatalog]);

    useEffect(() => {
        void fetchWeek();
    }, [fetchWeek]);

    useEffect(() => {
        const timer = window.setInterval(() => {
            if (!document.hidden) void fetchWeek();
        }, WEEK_MS);
        return () => window.clearInterval(timer);
    }, [fetchWeek]);

    useEffect(() => {
        let handle = 0;
        const schedule = () => {
            const midnight = new Date();
            midnight.setHours(24, 0, 0, 250);
            handle = window.setTimeout(
                () => {
                    setDayKey(todayKey());
                    if (followingToday) setViewDayRaw(todayKey());
                    void poll();
                    schedule();
                },
                midnight.getTime() - Date.now(),
            );
        };
        schedule();
        return () => window.clearTimeout(handle);
    }, [followingToday, poll]);

    return {
        phase,
        statusInfo,
        summary,
        catalog,
        lastError,
        todayKey: dayKey,
        viewDay,
        isViewingToday,
        week,
        weekLoading,
        refreshCatalog,
        setViewDay,
        goPrevDay,
        goNextDay,
        goToday,
    };
}
