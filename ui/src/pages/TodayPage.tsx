import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Lightbulb, Moon, Pencil, ShieldCheck, ShieldOff } from "lucide-react";
import { listManualBlocks } from "../api";
import type { BudgetHue } from "../budgetHue";
import { isHiddenDomain } from "../domains";
import { formatDayLabel, formatDuration, shiftDay, targetLabel } from "../format";
import { suggestBudget, type BudgetSuggestion } from "../activityModel";
import { Amount } from "../components/Amount";
import { backAtLabel, clockLabel, daysLabel, limitRule } from "../limitText";
import { useDowntime } from "../hooks/useDowntime";
import { useWeekDetail } from "../hooks/useWeekDetail";
import type { useLedgerActions } from "../hooks/useLedgerActions";
import {
    budgetStates,
    dayWindowStart,
    nowPosition,
    scheduleBands,
    scheduleOutlook,
    stripSegments,
    totalState,
    type BudgetState,
    type ScheduleBand,
    type StripSegment,
    type TotalState,
} from "../todayModel";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { StatusDto } from "../types/generated/StatusDto";

type Actions = ReturnType<typeof useLedgerActions>;

interface TodayPageProps {
    summary: DaySummaryDto | null;
    catalog: CatalogDto | null;
    statusInfo: StatusDto | null;
    viewDay: number;
    isViewingToday: boolean;
    loading: boolean;
    now: Date;
    actions: Actions;
    onOpenLimits: () => void;
    /** Shown under the hero (the blocked-apps banner). */
    banner?: ReactNode;
    /** Shown last (most used apps, the week). */
    more?: ReactNode;
}

const HUE_ORDER: BudgetHue[] = ["games", "social", "video", "other"];

/** A clock time in the UI language: "4 AM", "10:30 PM". */
function timeLabel(d: Date): string {
    return clockLabel(d.getHours() * 60 + d.getMinutes());
}

export function TodayPage({ summary, catalog, statusInfo, viewDay, isViewingToday, loading, now, actions, onOpenLimits, banner, more }: TodayPageProps) {
    const { t } = useTranslation();
    const dayStart = statusInfo?.day_start_minutes ?? 0;
    const { schedules } = useDowntime();

    const total = totalState(catalog, summary, viewDay);
    const budgets = useMemo(() => budgetStates(catalog, summary, viewDay, now), [catalog, summary, viewDay, now]);
    const segments = useMemo(() => stripSegments(summary, catalog, viewDay, dayStart), [summary, catalog, viewDay, dayStart]);
    const bands = useMemo(() => scheduleBands(schedules, viewDay, dayStart), [schedules, viewDay, dayStart]);
    const outlook = scheduleOutlook(schedules, viewDay, dayStart, now);
    const used = summary?.total_seconds ?? 0;

    // Headline: the most pressing budget first, then the total.
    const done = budgets.filter((b) => b.status === "done");
    const low = budgets.filter((b) => b.status === "low").sort((a, b) => a.left - b.left);
    const nameOf = (b: BudgetState) => targetLabel(b.limit.target, catalog);
    const firstDone = done[0];
    const firstLow = low[0];
    let title: string;
    if (!isViewingToday) title = formatDayLabel(viewDay);
    else if (total?.status === "done") title = t("today.titleTotalDone");
    else if (firstDone) title = t("today.titleDone", { name: nameOf(firstDone) });
    else if (total?.status === "low") title = t("today.titleTotalLow");
    else if (firstLow) title = t("today.titleLow", { name: nameOf(firstLow) });
    else if (budgets.length === 0 && total === null) title = t("today.titleNoBudgets");
    else title = t("today.titleOnTrack");

    let subtitle = isViewingToday
        ? t("today.usedSoFar", { amount: formatDuration(used) })
        : t("today.usedOnDay", { amount: formatDuration(used) });
    if (isViewingToday && outlook.active) {
        subtitle += " " + t("today.scheduleActive", { name: outlook.active.schedule.name, time: timeLabel(outlook.active.end) });
    } else if (isViewingToday && outlook.next) {
        const inSeconds = Math.round((outlook.next.start.getTime() - now.getTime()) / 1000);
        subtitle +=
            " " +
            t("today.scheduleNext", {
                name: outlook.next.schedule.name,
                time: timeLabel(outlook.next.start),
                in: formatDuration(Math.max(60, inSeconds - (inSeconds % 60))),
            });
    }

    const resetAt = dayWindowStart(viewDay, dayStart);
    resetAt.setDate(resetAt.getDate() + 1);

    return (
        <div className="tt-today">
            <section className="tt-card tt-today-hero" aria-labelledby="today-title">
                <Ring total={total} used={used} isToday={isViewingToday} loading={loading} />
                <div className="tt-today-hero-body">
                    <div className="tt-today-head">
                        <h1 id="today-title" className="tt-today-title">{title}</h1>
                        <p className="tt-sub">{subtitle}</p>
                    </div>
                    <DayStrip
                        segments={segments}
                        bands={bands}
                        viewDay={viewDay}
                        dayStart={dayStart}
                        now={isViewingToday ? nowPosition(viewDay, dayStart, now) : null}
                        catalog={catalog}
                        budgets={budgets}
                        used={used}
                    />
                </div>
            </section>

            {banner}

            {budgets.length > 0 ? (
                <section className="tt-tiles" aria-label={t("today.budgets")}>
                    {budgets.map((b) => (
                        <BudgetTile
                            key={b.limit.id}
                            state={b}
                            name={nameOf(b)}
                            isToday={isViewingToday}
                            resetAt={resetAt}
                            onEdit={() => actions.openEditor(b.limit.target, b.limit)}
                        />
                    ))}
                </section>
            ) : (
                catalog !== null && (
                    <section className="tt-card tt-today-empty">
                        <div>
                            <h2 className="tt-card-title">{t("today.noBudgetsTitle")}</h2>
                            <p className="tt-sub">{t("today.noBudgetsBody")}</p>
                        </div>
                        <button type="button" className="tt-btn tt-btn--primary" onClick={actions.startNewOrder}>
                            {t("today.addBudget")}
                        </button>
                    </section>
                )
            )}

            {isViewingToday && budgets.length + (total ? 1 : 0) > 0 && (
                <Suggestion
                    viewDay={viewDay}
                    catalog={catalog}
                    onAdd={(appId) => actions.openEditor({ kind: "app", id: appId }, null)}
                />
            )}

            <section className="tt-today-cards" aria-label={t("today.rulesLabel")}>
                <ScheduleCard outlook={outlook} schedules={schedules} onOpen={onOpenLimits} />
                <WebsitesCard familyDns={statusInfo?.family_dns_enabled ?? false} onOpen={onOpenLimits} />
            </section>

            {more}
        </div>
    );
}

// ------------------------------------------------------------------ ring

function Ring({ total, used, isToday, loading }: { total: TotalState | null; used: number; isToday: boolean; loading: boolean }) {
    const { t } = useTranslation();
    const r = 68;
    const c = 2 * Math.PI * r;
    const share = total && total.budget > 0 ? total.left / total.budget : 0;
    // Today's total is used up: say it in words, like the headline does.
    const usedUp = isToday && total !== null && total.budget > 0 && total.left <= 0;
    const label = total
        ? t("today.ringAria", { left: formatDuration(total.left), budget: formatDuration(total.budget) })
        : t("today.ringAriaUsed", { amount: formatDuration(used) });
    return (
        <div className="tt-ring" role="img" aria-label={loading ? undefined : label}>
            <svg viewBox="0 0 160 160" aria-hidden="true">
                <circle cx="80" cy="80" r={r} className="tt-ring-track" />
                {total && share > 0 && (
                    <circle
                        cx="80"
                        cy="80"
                        r={r}
                        className="tt-ring-arc"
                        strokeDasharray={`${c * share} ${c}`}
                        transform="rotate(-90 80 80)"
                    />
                )}
            </svg>
            <div className="tt-ring-value" aria-hidden="true">
                <span className={`tt-ring-number${usedUp ? " tt-ring-number--word" : ""}`}>
                    {usedUp ? t("today.ringDone") : <Amount seconds={total ? total.left : used} />}
                </span>
                <span className="tt-ring-caption">
                    {usedUp && total
                        ? t("today.ringDoneCaption", { budget: formatDuration(total.budget) })
                        : total
                        ? t("today.ringLeft", { budget: formatDuration(total.budget) })
                        : isToday
                          ? t("today.ringUsedToday")
                          : t("today.ringUsed")}
                </span>
            </div>
        </div>
    );
}

// ------------------------------------------------------------- day strip

function DayStrip({
    segments,
    bands,
    viewDay,
    dayStart,
    now,
    catalog,
    budgets,
    used,
}: {
    segments: StripSegment[];
    bands: ScheduleBand[];
    viewDay: number;
    dayStart: number;
    now: number | null;
    catalog: CatalogDto | null;
    budgets: BudgetState[];
    used: number;
}) {
    const { t } = useTranslation();
    const start = dayWindowStart(viewDay, dayStart);
    const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => ({
        f,
        label: timeLabel(new Date(start.getTime() + f * 86_400_000)),
    }));
    const hues = HUE_ORDER.filter((h) => segments.some((s) => s.hue === h));
    const legendName = (hue: BudgetHue) => {
        if (hue === "other") return t("today.everythingElse");
        const names = budgets.filter((b) => b.hue === hue).map((b) => targetLabel(b.limit.target, catalog));
        return names.length > 0 ? names.join(", ") : t("today.everythingElse");
    };
    const scheduleNames = [...new Set(bands.map((b) => b.schedule.name))];
    const description = t("today.stripAria", {
        from: timeLabel(start),
        used: formatDuration(used),
    });

    return (
        <div className="tt-strip-wrap">
            <div className="tt-strip" role="img" aria-label={description}>
                {bands.map((b, i) => (
                    <span
                        key={`b${i}`}
                        className="tt-strip-band"
                        style={{ insetInlineStart: `${b.from * 100}%`, width: `${(b.to - b.from) * 100}%` }}
                    />
                ))}
                {segments.map((s, i) => (
                    <span
                        key={i}
                        className={`tt-strip-seg tt-hue-${s.hue}`}
                        style={{ insetInlineStart: `${s.from * 100}%`, width: `${(s.to - s.from) * 100}%` }}
                    />
                ))}
                {now !== null && <span className="tt-strip-now" style={{ insetInlineStart: `${now * 100}%` }} />}
            </div>
            <div className="tt-strip-scale" aria-hidden="true">
                {ticks
                    .filter((tick) => now === null || Math.abs(tick.f - now) > 0.08)
                    .map((tick) => (
                        <span
                            key={tick.f}
                            className={tick.f === 0 ? "tt-strip-tick tt-strip-tick--start" : tick.f === 1 ? "tt-strip-tick tt-strip-tick--end" : "tt-strip-tick"}
                            style={tick.f === 0 || tick.f === 1 ? undefined : { insetInlineStart: `${tick.f * 100}%` }}
                        >
                            {tick.label}
                        </span>
                    ))}
                {now !== null && (
                    <span className="tt-strip-tick tt-strip-tick--now" style={{ insetInlineStart: `${now * 100}%` }}>
                        {t("today.now")}
                    </span>
                )}
            </div>
            {(hues.length > 0 || scheduleNames.length > 0) && (
                <ul className="tt-strip-legend">
                    {hues.map((h) => (
                        <li key={h} className={`tt-hue-${h}`}>
                            <span className="tt-strip-key" aria-hidden="true" />
                            {legendName(h)}
                        </li>
                    ))}
                    {scheduleNames.map((name) => (
                        <li key={`s-${name}`}>
                            <span className="tt-strip-key tt-strip-key--band" aria-hidden="true" />
                            {name}
                        </li>
                    ))}
                </ul>
            )}
        </div>
    );
}

// ----------------------------------------------------------------- tiles

function BudgetTile({
    state,
    name,
    isToday,
    resetAt,
    onEdit,
}: {
    state: BudgetState;
    name: string;
    isToday: boolean;
    resetAt: Date;
    onEdit: () => void;
}) {
    const { t } = useTranslation();
    const fill = state.budget > 0 ? Math.min(1, state.left / state.budget) : 0;
    const status = isToday ? state.status : null;
    const pill =
        status === "low"
            ? t("today.statusLow")
            : status === "done"
              ? t("today.statusDone")
              : status === "extra"
                ? t("today.statusExtra")
                : status === "plenty"
                  ? t("today.statusPlenty")
                  : null;
    return (
        <article className={`tt-tile tt-hue-${state.hue}${status ? ` tt-tile--${status}` : ""}`} aria-labelledby={`tile-${state.limit.id}`}>
            {status !== "done" && <span className="tt-tile-fill" style={{ height: `${fill * 100}%` }} aria-hidden="true" />}
            <div className="tt-tile-top">
                <div className="tt-tile-name">
                    <h2 id={`tile-${state.limit.id}`}>{name}</h2>
                    <span>{limitRule(state.limit)}</span>
                </div>
                {pill && <span className={`tt-tile-pill tt-tile-pill--${status}`}>{pill}</span>}
            </div>
            <div className="tt-tile-bottom">
                <div className="tt-tile-figure">
                    {!isToday ? (
                        <>
                            <span className="tt-tile-number">
                                <Amount seconds={state.used} />
                                <small> {t("today.used")}</small>
                            </span>
                            <span className="tt-tile-apps">{t("today.ofBudget", { budget: formatDuration(state.budget) })}</span>
                        </>
                    ) : status === "done" ? (
                        <>
                            <span className="tt-tile-number tt-tile-number--text">{t("today.backAt", { time: backAtLabel(resetAt) })}</span>
                            {state.apps.length > 0 && <span className="tt-tile-apps">{state.apps.join(", ")}</span>}
                        </>
                    ) : (
                        <>
                            <span className="tt-tile-number">
                                <Amount seconds={state.left} />
                                <small> {t("today.left")}</small>
                            </span>
                            <span className="tt-tile-apps">
                                {status === "extra" && state.extraUntil
                                    ? t("today.extraUntil", { time: timeLabel(state.extraUntil) })
                                    : state.apps.join(", ")}
                            </span>
                        </>
                    )}
                </div>
                <button type="button" className="tt-tile-edit" onClick={onEdit} aria-label={t("today.editBudget", { name })} title={t("today.editBudget", { name })}>
                    <Pencil size={16} aria-hidden="true" />
                </button>
            </div>
        </article>
    );
}

// ------------------------------------------------------------ suggestion

const DISMISSED_KEY = "tether.suggestionDismissed";

/** App id → the day key it was dismissed on. Per viewer, best effort. */
function readDismissed(): Record<string, number> {
    try {
        const raw = window.localStorage.getItem(DISMISSED_KEY);
        const parsed: unknown = raw ? JSON.parse(raw) : {};
        return parsed && typeof parsed === "object" ? (parsed as Record<string, number>) : {};
    } catch {
        return {};
    }
}

function writeDismissed(map: Record<string, number>) {
    try {
        window.localStorage.setItem(DISMISSED_KEY, JSON.stringify(map));
    } catch {
        // Storage unavailable: the card just comes back next time.
    }
}

/**
 * "From last week": the most used app no budget covers (activityModel
 * `suggestBudget`), offered as a new budget. "Not now" hides that app's
 * suggestion for a week.
 */
function Suggestion({ viewDay, catalog, onAdd }: { viewDay: number; catalog: CatalogDto | null; onAdd: (appId: number) => void }) {
    const { t } = useTranslation();
    const { summaries, loading } = useWeekDetail(shiftDay(viewDay, -1));
    const [dismissed, setDismissed] = useState(readDismissed);
    const suggestion: BudgetSuggestion | null = useMemo(() => suggestBudget(catalog, summaries), [catalog, summaries]);
    if (loading || !suggestion) return null;
    const when = dismissed[String(suggestion.appId)];
    if (typeof when === "number" && when <= viewDay && viewDay < shiftDay(when, 7)) return null;

    const dismiss = () => {
        const next = { ...dismissed, [String(suggestion.appId)]: viewDay };
        writeDismissed(next);
        setDismissed(next);
    };
    return (
        <section className="tt-card tt-suggest" aria-labelledby="today-suggest">
            <span className="tt-today-card-icon tt-today-card-icon--plum" aria-hidden="true">
                <Lightbulb size={20} />
            </span>
            <div className="tt-suggest-text">
                <h2 id="today-suggest" className="tt-suggest-eyebrow">{t("today.suggestEyebrow")}</h2>
                <p>{t("today.suggestBody", { name: suggestion.label, amount: formatDuration(suggestion.avgSeconds) })}</p>
            </div>
            <div className="tt-suggest-actions">
                <button type="button" className="tt-btn tt-btn--outline" onClick={dismiss}>
                    {t("today.suggestLater")}
                </button>
                <button type="button" className="tt-btn tt-btn--primary" onClick={() => onAdd(suggestion.appId)}>
                    {t("today.suggestAdd")}
                </button>
            </div>
        </section>
    );
}

// ----------------------------------------------------------------- cards

function ScheduleCard({
    outlook,
    schedules,
    onOpen,
}: {
    outlook: { active: ScheduleBand | null; next: ScheduleBand | null };
    schedules: { name: string; start_minute: number; end_minute: number; weekday_mask: number; enabled: boolean }[];
    onOpen: () => void;
}) {
    const { t } = useTranslation();
    const shown = outlook.active?.schedule ?? outlook.next?.schedule ?? schedules.find((s) => s.enabled) ?? null;
    return (
        <button type="button" className="tt-today-card" onClick={onOpen}>
            <span className="tt-today-card-icon tt-today-card-icon--plum" aria-hidden="true">
                <Moon size={20} />
            </span>
            <span className="tt-today-card-text">
                {shown ? (
                    <>
                        <strong>
                            {t("today.scheduleRange", {
                                name: shown.name,
                                from: clockLabel(shown.start_minute),
                                to: clockLabel(shown.end_minute),
                            })}
                        </strong>
                        <span>{outlook.active ? t("today.scheduleOnNow") : daysLabel(shown.weekday_mask)}</span>
                    </>
                ) : (
                    <>
                        <strong>{t("today.noSchedules")}</strong>
                        <span>{t("today.noSchedulesHint")}</span>
                    </>
                )}
            </span>
        </button>
    );
}

function WebsitesCard({ familyDns, onOpen }: { familyDns: boolean; onOpen: () => void }) {
    const { t } = useTranslation();
    const [count, setCount] = useState<number | null>(null);
    useEffect(() => {
        let live = true;
        listManualBlocks()
            .then((r) => live && setCount(r.domains.filter((d) => !isHiddenDomain(d)).length))
            .catch(() => live && setCount(null));
        return () => {
            live = false;
        };
    }, []);
    const filtered = familyDns || (count ?? 0) > 0;
    const sites = count ? t("today.sitesBlocked", { count }) : "";
    return (
        <button type="button" className="tt-today-card" onClick={onOpen}>
            <span className={`tt-today-card-icon ${filtered ? "tt-today-card-icon--ok" : ""}`} aria-hidden="true">
                {filtered ? <ShieldCheck size={20} /> : <ShieldOff size={20} />}
            </span>
            <span className="tt-today-card-text">
                <strong>{filtered ? t("today.websitesFiltered") : t("today.websitesOpen")}</strong>
                <span>
                    {familyDns
                        ? [sites, t("today.familyDnsOn")].filter(Boolean).join(" · ")
                        : filtered
                          ? sites
                          : t("today.websitesHint")}
                </span>
            </span>
        </button>
    );
}
