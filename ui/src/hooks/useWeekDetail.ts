import { useEffect, useMemo, useState } from "react";
import { blockReasons, getDaySummary, getWeeklySummary } from "../api";
import { weekDays } from "../activityModel";
import type { BlockReasonCountDto } from "../types/generated/BlockReasonCountDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { WeeklySummaryDto } from "../types/generated/WeeklySummaryDto";

export interface WeekDetail {
    days: number[];
    week: WeeklySummaryDto | null;
    /** Day summaries that loaded, by day key. */
    summaries: Map<number, DaySummaryDto>;
    reasons: BlockReasonCountDto[];
    loading: boolean;
}

/**
 * The seven days ending at `endDay`: weekly totals, each day's summary and
 * the block-screen reasons. Shared by Activity and Today's suggestion.
 * Pass `enabled = false` to skip loading.
 */
export function useWeekDetail(endDay: number, enabled = true): WeekDetail {
    const days = useMemo(() => weekDays(endDay), [endDay]);
    const [week, setWeek] = useState<WeeklySummaryDto | null>(null);
    const [summaries, setSummaries] = useState<Map<number, DaySummaryDto>>(new Map());
    const [reasons, setReasons] = useState<BlockReasonCountDto[]>([]);
    const [loading, setLoading] = useState(enabled);

    useEffect(() => {
        if (!enabled) return;
        let live = true;
        setLoading(true);
        (async () => {
            const [w, why, ...daySummaries] = await Promise.all([
                getWeeklySummary(endDay).catch(() => null),
                blockReasons(days[0] ?? endDay, endDay).catch(() => null),
                ...days.map((d) => getDaySummary(d).catch(() => null)),
            ]);
            if (!live) return;
            const map = new Map<number, DaySummaryDto>();
            daySummaries.forEach((s, i) => {
                const day = days[i];
                if (s && day !== undefined) map.set(day, s);
            });
            setWeek(w);
            setReasons(why?.counts ?? []);
            setSummaries(map);
            setLoading(false);
        })();
        return () => {
            live = false;
        };
    }, [endDay, days, enabled]);

    return { days, week, summaries, reasons, loading };
}
