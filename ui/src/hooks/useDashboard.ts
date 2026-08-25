import { useCallback, useEffect, useRef, useState } from "react";

import { describeError, getCatalog, getDaySummary, getStatus } from "../api";
import { todayKey } from "../format";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { StatusDto } from "../types/generated/StatusDto";

export type Phase = "connecting" | "live" | "offline";

// One poll per visible second: a local named-pipe round trip plus one small
// SQLite read is well under a millisecond on the agent side, and 1 Hz is what
// makes the ledger feel like it is counting rather than refreshing.
const POLL_MS = 1000;
const CATALOG_MS = 60000;

export interface Dashboard {
    phase: Phase;
    statusInfo: StatusDto | null;
    summary: DaySummaryDto | null;
    catalog: CatalogDto | null;
    lastError: string | null;
    todayKey: number;
    refreshCatalog: () => void;
}

export function useDashboard(): Dashboard {
    const [phase, setPhase] = useState<Phase>("connecting");
    const [statusInfo, setStatusInfo] = useState<StatusDto | null>(null);
    const [summary, setSummary] = useState<DaySummaryDto | null>(null);
    const [catalog, setCatalog] = useState<CatalogDto | null>(null);
    const [lastError, setLastError] = useState<string | null>(null);
    const [dayKey, setDayKey] = useState<number>(() => todayKey());

    const pollSeq = useRef(0);
    const catalogSeq = useRef(0);

    const poll = useCallback(async () => {
        const ticket = ++pollSeq.current;
        try {
            const status = await getStatus();
            if (ticket !== pollSeq.current) return;
            setStatusInfo(status);
            const nextSummary = await getDaySummary(todayKey());
            if (ticket !== pollSeq.current) return;
            setSummary(nextSummary);
            setPhase("live");
            setLastError(null);
        } catch (error) {
            if (ticket !== pollSeq.current) return;
            setPhase("offline");
            setLastError(describeError(error));
        }
    }, []);

    const refreshCatalog = useCallback(() => {
        const ticket = ++catalogSeq.current;
        getCatalog()
            .then((next) => {
                if (ticket === catalogSeq.current) setCatalog(next);
            })
            .catch(() => {});
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
        let handle = 0;
        const schedule = () => {
            const midnight = new Date();
            midnight.setHours(24, 0, 0, 250);
            handle = window.setTimeout(
                () => {
                    setDayKey(todayKey());
                    void poll();
                    schedule();
                },
                midnight.getTime() - Date.now(),
            );
        };
        schedule();
        return () => window.clearTimeout(handle);
    }, [poll]);

    return { phase, statusInfo, summary, catalog, lastError, todayKey: dayKey, refreshCatalog };
}
