import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Clock3 } from "lucide-react";
import i18n from "../i18n";
import { addManualBlock, describeError, listManualBlocks } from "../api";
import { hueFor } from "../budgetHue";
import { isHiddenDomain } from "../domains";
import { formatDuration, formatWhen, targetLabel, weekdayShortNames } from "../format";
import { useDowntime } from "../hooks/useDowntime";
import type { useLedgerActions } from "../hooks/useLedgerActions";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import type { StatusDto } from "../types/generated/StatusDto";
import { DowntimeSection } from "../components/DowntimeSection";
import { WebFilteringPanel } from "../components/WebFilteringPanel";

type Actions = ReturnType<typeof useLedgerActions>;
type Notify = (kind: "success" | "error" | "info", message: string) => void;

interface LimitsPageProps {
    catalog: CatalogDto | null;
    summary: DaySummaryDto | null;
    statusInfo: StatusDto | null;
    actions: Actions;
    notify: Notify;
    onSetSetting: (key: string, value: string) => Promise<void>;
    settingPending: (key: string) => boolean;
}

/** Shown when the full Websites or Schedules view is open. */
type View = "main" | "schedules" | "websites";

const DOMAINS_IN_CARD = 3;

/** "10 pm" / "22:00", following the UI language. */
function clockLabel(minute: number): string {
    const d = new Date(2024, 0, 1, Math.floor(minute / 60), minute % 60);
    return d.toLocaleTimeString(i18n.language, { hour: "numeric", minute: minute % 60 === 0 ? undefined : "2-digit" });
}

/** Which days a schedule runs (Monday-first bit mask). */
function daysLabel(mask: number): string {
    if ((mask & 127) === 127) return i18n.t("limitsPage.everyDay");
    if ((mask & 127) === 31) return i18n.t("limitsPage.weekdays");
    if ((mask & 127) === 96) return i18n.t("limitsPage.weekends");
    const names = weekdayShortNames();
    return names.filter((_, i) => (mask & (1 << i)) !== 0).join(" · ");
}

/** How a limit is set, in words: "1h every day", "1h · Sat 2h · Sun 2h". */
function limitRule(limit: LimitDto): string {
    const base = formatDuration(limit.default_minutes * 60);
    const names = weekdayShortNames();
    const overrides = limit.weekday_minutes
        .map((m, i) => (m === null ? null : `${names[i]} ${formatDuration(m * 60)}`))
        .filter((x): x is string => x !== null);
    return overrides.length === 0
        ? i18n.t("limitsPage.everyDayAmount", { amount: base })
        : [base, ...overrides].join(" · ");
}

/** Today's use against today's budget for one limit. */
function todayUse(limit: LimitDto, summary: DaySummaryDto | null): { used: number; budget: number } {
    const mondayFirst = (new Date().getDay() + 6) % 7;
    const own = (limit.weekday_minutes[mondayFirst] ?? limit.default_minutes) * 60;
    if (limit.target.kind === "total") return { used: summary?.total_seconds ?? 0, budget: own };
    const targetId = limit.target.id;
    const rows = limit.target.kind === "app" ? summary?.apps : summary?.categories;
    const row = rows?.find((x) => x.id === targetId);
    return { used: row?.seconds ?? 0, budget: row?.limit_seconds ?? own };
}

export function LimitsPage({ catalog, summary, statusInfo, actions, notify, onSetSetting, settingPending }: LimitsPageProps) {
    const { t } = useTranslation();
    const [view, setView] = useState<View>("main");
    const downtime = useDowntime(notify, actions.guarded);

    if (view === "schedules" || view === "websites") {
        return (
            <div className="tt-page">
                <button type="button" className="tt-btn tt-btn--outline tt-btn--sm tt-back" onClick={() => setView("main")}>
                    {t("limitsPage.backToLimits")}
                </button>
                {view === "schedules" ? (
                    <>
                        <h1 className="tt-title">{t("limitsPage.schedules")}</h1>
                        <DowntimeSection catalog={catalog} notify={notify} guarded={actions.guarded} />
                    </>
                ) : (
                    <WebFilteringPanel onAttempt={actions.attempt} />
                )}
            </div>
        );
    }

    const cooldown = statusInfo?.limit_cooldown_hours ?? 24;
    const limits = catalog?.limits ?? [];
    const pending = catalog?.pending_limits ?? [];

    return (
        <div className="tt-page">
            <div className="tt-head">
                <div>
                    <h1 className="tt-title">{t("limitsPage.title")}</h1>
                    <p className="tt-sub">
                        {cooldown > 0 ? t("limitsPage.subtitle", { count: cooldown }) : t("limitsPage.subtitleNoWait")}
                    </p>
                </div>
                <button type="button" className="tt-btn tt-btn--primary" onClick={actions.startNewOrder} disabled={actions.busy}>
                    {t("limitsPage.newBudget")}
                </button>
            </div>

            {pending.map((p) => {
                const name = targetLabel(p.target, catalog);
                const when = formatWhen(new Date(p.effective_from_utc));
                const text =
                    p.action === "delete"
                        ? t("limitsPage.waitingRemove", { name, when })
                        : t("limitsPage.waitingChange", {
                              name,
                              amount: formatDuration((p.default_minutes ?? 0) * 60),
                              when,
                          });
                return (
                    <div className="tt-banner" role="status" key={p.id}>
                        <span>
                            <Clock3 size={18} className="tt-note-icon" aria-hidden="true" />
                            <span>
                                <strong>{t("limitsPage.waiting")}</strong> {text}
                            </span>
                        </span>
                        <button
                            type="button"
                            className="tt-btn tt-btn--outline tt-btn--sm"
                            onClick={() => actions.cancelPendingLimit(p.target)}
                            disabled={actions.busy}
                        >
                            {t("limitsPage.cancelChange")}
                        </button>
                    </div>
                );
            })}

            <div className="tt-columns">
                <section className="tt-card tt-col-main" aria-labelledby="limits-budgets">
                    <h2 id="limits-budgets" className="tt-card-title">{t("limitsPage.budgets")}</h2>
                    {catalog === null ? (
                        <p className="tt-row-detail">{t("limitsPage.loading")}</p>
                    ) : limits.length === 0 ? (
                        <div className="tt-row">
                            <span className="tt-row-text">
                                <span className="tt-row-title">{t("limitsPage.noBudgets")}</span>
                                <span className="tt-row-detail">{t("limitsPage.noBudgetsHint")}</span>
                            </span>
                        </div>
                    ) : (
                        <ul className="tt-list">
                            {limits.map((limit) => (
                                <BudgetRow
                                    key={limit.id}
                                    limit={limit}
                                    catalog={catalog}
                                    summary={summary}
                                    busy={actions.busy}
                                    onEdit={() => actions.openEditor(limit.target, limit)}
                                    onToggle={(next) => actions.toggleLimit(limit, next)}
                                />
                            ))}
                        </ul>
                    )}
                </section>

                <div className="tt-col-side">
                    <SchedulesCard schedules={downtime.schedules} loading={downtime.loading} onToggle={downtime.toggleSchedule} onManage={() => setView("schedules")} />
                    <WebsitesCard
                        familyDns={statusInfo?.family_dns_enabled ?? false}
                        familyDnsPending={settingPending("family_dns")}
                        onFamilyDns={(on) => onSetSetting("family_dns", String(on))}
                        onManage={() => setView("websites")}
                    />
                </div>
            </div>
        </div>
    );
}

function BudgetRow({
    limit,
    catalog,
    summary,
    busy,
    onEdit,
    onToggle,
}: {
    limit: LimitDto;
    catalog: CatalogDto | null;
    summary: DaySummaryDto | null;
    busy: boolean;
    onEdit: () => void;
    onToggle: (next: boolean) => void;
}) {
    const { t } = useTranslation();
    const name = targetLabel(limit.target, catalog);
    const { used, budget } = todayUse(limit, summary);
    const over = limit.enabled && budget > 0 && used >= budget;
    const share = budget > 0 ? Math.min(100, Math.round((used / budget) * 100)) : 0;
    return (
        <li className={`tt-budget tt-hue-${hueFor(limit.target, catalog)} ${limit.enabled ? "" : "tt-budget--off"}`}>
            <span className="tt-swatch" aria-hidden="true">
                <span />
            </span>
            <span className="tt-row-text">
                <span className="tt-row-title">{name}</span>
                <span className="tt-row-detail">{limit.enabled ? limitRule(limit) : t("limitsPage.paused")}</span>
            </span>
            <span className="tt-meter-block">
                <span className={over ? "tt-over" : undefined}>
                    {over
                        ? t("limitsPage.usedUp")
                        : t("limitsPage.usedOf", { used: formatDuration(used), budget: formatDuration(budget) })}
                </span>
                <span
                    className="tt-meter"
                    role="progressbar"
                    aria-label={t("limitsPage.usedToday", { name })}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={share}
                >
                    <span style={{ width: `${share}%` }} />
                </span>
            </span>
            <span className="tt-row-end">
                <input
                    type="checkbox"
                    role="switch"
                    className="toggle-switch"
                    checked={limit.enabled}
                    disabled={busy}
                    aria-label={t("limits.toggleLabel", { target: name })}
                    onChange={(e) => onToggle(e.target.checked)}
                />
                <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" onClick={onEdit} disabled={busy}>
                    {t("limitsPage.edit")}
                </button>
            </span>
        </li>
    );
}

function SchedulesCard({
    schedules,
    loading,
    onToggle,
    onManage,
}: {
    schedules: ScheduleDto[];
    loading: boolean;
    onToggle: (id: number, enabled: boolean) => Promise<void>;
    onManage: () => void;
}) {
    const { t } = useTranslation();
    return (
        <section className="tt-card" aria-labelledby="limits-schedules">
            <div className="tt-card-head">
                <h2 id="limits-schedules" className="tt-card-title">{t("limitsPage.schedules")}</h2>
                <button type="button" className="tt-link" onClick={onManage}>
                    {t("limitsPage.manage")}
                </button>
            </div>
            {loading && schedules.length === 0 ? (
                <p className="tt-row-detail">{t("limitsPage.loading")}</p>
            ) : schedules.length === 0 ? (
                <p className="tt-row-detail">{t("limitsPage.noSchedules")}</p>
            ) : (
                <ul className="tt-list">
                    {schedules.map((s) => (
                        <li className="tt-row" key={s.id}>
                            <span className="tt-row-text">
                                <span className="tt-row-title">{s.name}</span>
                                <span className="tt-row-detail">
                                    {clockLabel(s.start_minute)} – {clockLabel(s.end_minute)} · {daysLabel(s.weekday_mask)}
                                </span>
                            </span>
                            <input
                                type="checkbox"
                                role="switch"
                                className="toggle-switch"
                                checked={s.enabled}
                                aria-label={t("downtime.toggleLabel", { name: s.name })}
                                onChange={(e) => void onToggle(s.id, e.target.checked)}
                            />
                        </li>
                    ))}
                </ul>
            )}
            <p className="tt-row-detail">{t("limitsPage.schedulesHint")}</p>
        </section>
    );
}

function WebsitesCard({
    familyDns,
    familyDnsPending,
    onFamilyDns,
    onManage,
}: {
    familyDns: boolean;
    familyDnsPending: boolean;
    onFamilyDns: (on: boolean) => Promise<void>;
    onManage: () => void;
}) {
    const { t } = useTranslation();
    const [domains, setDomains] = useState<string[] | null>(null);
    const [draft, setDraft] = useState("");
    const [error, setError] = useState<string | null>(null);

    const refresh = () => {
        listManualBlocks()
            .then((res) => setDomains(res.domains))
            .catch((e) => setError(describeError(e)));
    };
    useEffect(refresh, []);

    const add = async (e: FormEvent) => {
        e.preventDefault();
        const domain = draft.trim().toLowerCase();
        if (!domain) return;
        setError(null);
        try {
            await addManualBlock(domain);
            setDraft("");
            refresh();
        } catch (err) {
            setError(describeError(err));
        }
    };

    const visible = (domains ?? []).filter((d) => !isHiddenDomain(d));
    const hidden = (domains ?? []).length - visible.length;
    const shown = visible.slice(0, DOMAINS_IN_CARD);
    const more = visible.length - shown.length + hidden;

    return (
        <section className="tt-card" aria-labelledby="limits-websites">
            <div className="tt-card-head">
                <h2 id="limits-websites" className="tt-card-title">{t("limitsPage.websites")}</h2>
                <button type="button" className="tt-link" onClick={onManage}>
                    {t("limitsPage.manage")}
                </button>
            </div>
            <form className="tt-inline-form" onSubmit={add}>
                <label htmlFor="limits-site" className="tt-sr-only">{t("limitsPage.siteLabel")}</label>
                <input
                    id="limits-site"
                    className="tt-input"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    placeholder={t("limitsPage.sitePlaceholder")}
                    autoComplete="off"
                    spellCheck={false}
                />
                <button type="submit" className="tt-btn tt-btn--primary tt-btn--sm">{t("limitsPage.block")}</button>
            </form>
            {error !== null && <p className="tt-error" role="alert">{error}</p>}
            {domains !== null && domains.length > 0 && (
                <div className="tt-chips">
                    {shown.map((d) => (
                        <span className="tt-chip" key={d}>{d}</span>
                    ))}
                    {more > 0 && (
                        <button type="button" className="tt-link" onClick={onManage}>
                            {t("limitsPage.more", { count: more })}
                        </button>
                    )}
                </div>
            )}
            <div className="tt-row">
                <span className="tt-row-text">
                    <span className="tt-row-title">{t("limitsPage.familyDns")}</span>
                    <span className="tt-row-detail">{t("limitsPage.familyDnsHint")}</span>
                </span>
                <input
                    type="checkbox"
                    role="switch"
                    className="toggle-switch"
                    checked={familyDns}
                    disabled={familyDnsPending}
                    aria-label={t("limitsPage.familyDns")}
                    onChange={(e) => void onFamilyDns(e.target.checked)}
                />
            </div>
        </section>
    );
}
