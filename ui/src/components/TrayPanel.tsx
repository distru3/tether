import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCatalog, getDaySummary, getStatus, listSchedules } from "../api";
import { formatDuration, setDayStartMinutes, targetLabel, todayKey } from "../format";
import { clockLabel } from "../limitText";
import { useNowMinute } from "../hooks/useNowMinute";
import { useTheme } from "../hooks/useTheme";
import { budgetStates, dayWindowStart, scheduleOutlook, totalState, type BudgetState } from "../todayModel";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import type { StatusDto } from "../types/generated/StatusDto";
import { Amount } from "./Amount";

interface PanelData {
    status: StatusDto | null;
    catalog: CatalogDto | null;
    summary: DaySummaryDto | null;
    schedules: ScheduleDto[];
    offline: boolean;
}

function timeOf(d: Date): string {
    return clockLabel(d.getHours() * 60 + d.getMinutes());
}

/**
 * The tray panel (window `tray`, `?view=tray`): a glance at today from the
 * notification area. Opens on a left click on the tray icon, reloads every
 * time it opens (`tray_panel_shown`) and every 30 s while open, and hides on
 * Escape or when it loses focus (the host does that).
 */
export function TrayPanel() {
    const { t, i18n } = useTranslation();
    useTheme();
    const now = useNowMinute();
    const [data, setData] = useState<PanelData>({ status: null, catalog: null, summary: null, schedules: [], offline: false });

    const load = useCallback(async () => {
        try {
            const status = await getStatus();
            setDayStartMinutes(status.day_start_minutes);
            const [catalog, summary, schedules] = await Promise.all([
                getCatalog().catch(() => null),
                getDaySummary(todayKey()).catch(() => null),
                listSchedules().catch(() => null),
            ]);
            setData({ status, catalog, summary, schedules: schedules?.schedules ?? [], offline: false });
        } catch {
            setData((d) => ({ ...d, offline: true }));
        }
    }, []);

    useEffect(() => {
        // The window itself is transparent; the panel draws its own card.
        document.documentElement.style.background = "transparent";
        document.body.style.background = "transparent";
        void load();
        const timer = window.setInterval(() => void load(), 30_000);
        let unlisten: (() => void) | undefined;
        let live = true;
        listen("tray_panel_shown", () => void load())
            .then((f) => (live ? (unlisten = f) : f()))
            .catch(() => {});
        const onKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                try {
                    void getCurrentWindow().hide();
                } catch {
                    // Not running inside Tauri.
                }
            }
        };
        window.addEventListener("keydown", onKey);
        return () => {
            live = false;
            unlisten?.();
            window.clearInterval(timer);
            window.removeEventListener("keydown", onKey);
        };
    }, [load]);

    const { status, catalog, summary, schedules, offline } = data;
    const dayKey = todayKey(now);
    const dayStart = status?.day_start_minutes ?? 0;
    const total = totalState(catalog, summary, dayKey);
    const budgets = budgetStates(catalog, summary, dayKey, now);
    const outlook = scheduleOutlook(schedules, dayKey, dayStart, now);
    const used = summary?.total_seconds ?? 0;
    const reset = dayWindowStart(dayKey, dayStart);
    reset.setDate(reset.getDate() + 1);

    const scheduleLine = outlook.active
        ? t("trayPanel.scheduleUntil", { name: outlook.active.schedule.name, time: timeOf(outlook.active.end) })
        : outlook.next
          ? t("trayPanel.scheduleAt", { name: outlook.next.schedule.name, time: timeOf(outlook.next.start) })
          : null;

    const r = 32;
    const c = 2 * Math.PI * r;
    const share = total && total.budget > 0 ? total.left / total.budget : 0;

    const openDashboard = () => {
        invoke("open_dashboard").catch(() => {});
    };

    return (
        <main className="tt-tray" dir={i18n.dir()} aria-label="Tether">
            <div className="tt-tray-head">
                <div
                    className="tt-tray-ring"
                    role="img"
                    aria-label={
                        total
                            ? t("today.ringAria", { left: formatDuration(total.left), budget: formatDuration(total.budget) })
                            : t("today.ringAriaUsed", { amount: formatDuration(used) })
                    }
                >
                    <svg viewBox="0 0 78 78" aria-hidden="true">
                        <circle cx="39" cy="39" r={r} className="tt-ring-track" />
                        {total && share > 0 && (
                            <circle cx="39" cy="39" r={r} className="tt-ring-arc" strokeDasharray={`${c * share} ${c}`} transform="rotate(-90 39 39)" />
                        )}
                    </svg>
                    <span aria-hidden="true">
                        <Amount seconds={total ? total.left : used} />
                    </span>
                </div>
                <div className="tt-tray-title">
                    <strong>{total ? t("trayPanel.leftToday") : t("trayPanel.usedToday")}</strong>
                    <span>{[t("trayPanel.used", { amount: formatDuration(used) }), scheduleLine].filter(Boolean).join(" · ")}</span>
                </div>
            </div>

            {offline ? (
                <p className="tt-tray-note">{t("trayPanel.offline")}</p>
            ) : budgets.length === 0 ? (
                <p className="tt-tray-note">{t("trayPanel.noBudgets")}</p>
            ) : (
                <ul className="tt-tray-list">
                    {budgets.map((b) => (
                        <BudgetLine key={b.limit.id} state={b} name={targetLabel(b.limit.target, catalog)} reset={reset} />
                    ))}
                </ul>
            )}

            <button type="button" className="tt-tray-open" onClick={openDashboard}>
                {t("trayPanel.open")}
            </button>
        </main>
    );
}

function BudgetLine({ state, name, reset }: { state: BudgetState; name: string; reset: Date }) {
    const { t } = useTranslation();
    const fill = state.budget > 0 ? Math.min(1, state.left / state.budget) : 0;
    const text =
        state.status === "done"
            ? t("trayPanel.done", { time: timeOf(reset) })
            : state.status === "extra" && state.extraUntil
              ? t("trayPanel.extra", { time: timeOf(state.extraUntil) })
              : t("trayPanel.left", { amount: formatDuration(state.left) });
    return (
        <li className={`tt-tray-row tt-hue-${state.hue} tt-tray-row--${state.status}`}>
            <span className="tt-tray-row-line">
                <span>{name}</span>
                <span className="tt-tray-row-state">{text}</span>
            </span>
            <span className="tt-tray-bar" aria-hidden="true">
                <span style={{ width: `${fill * 100}%` }} />
            </span>
        </li>
    );
}
