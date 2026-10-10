import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Plus, Trash2, X } from "lucide-react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import { useDowntime } from "../hooks/useDowntime";
import { clockLabel, daysLabel } from "../limitText";
import { weekdayShortNames } from "../format";
import type { Guarded } from "../hooks/useLedgerActions";
import { Dialog } from "./Dialog";
import { LoadingSpinner } from "./LoadingSpinner";

interface DowntimeSectionProps {
    catalog: CatalogDto | null;
    notify: (kind: "success" | "error", message: string) => void;
    guarded?: Guarded;
}

/** Monday-first bit masks for the quick day picks. */
const EVERY_DAY = 127;
const WEEKDAYS = 31;
const WEEKENDS = 96;

function minuteToTimeString(min: number): string {
    const h = Math.floor(min / 60);
    const m = min % 60;
    return `${h.toString().padStart(2, "0")}:${m.toString().padStart(2, "0")}`;
}

function timeStringToMinute(timeStr: string): number {
    const [hStr, mStr] = timeStr.split(":");
    const h = parseInt(hStr || "0", 10);
    const m = parseInt(mStr || "0", 10);
    return Math.min(1439, Math.max(0, h * 60 + m));
}

/**
 * Whether a schedule is in force at `now`. An overnight schedule belongs to
 * the day it starts on, as in `st_core::schedules`.
 */
function isOnNow(schedule: ScheduleDto, now: Date): boolean {
    if (!schedule.enabled) return false;
    const day = (now.getDay() + 6) % 7; // Monday = 0
    const minute = now.getHours() * 60 + now.getMinutes();
    const runsOn = (d: number) => (schedule.weekday_mask & (1 << d)) !== 0;
    if (schedule.start_minute <= schedule.end_minute) {
        return runsOn(day) && minute >= schedule.start_minute && minute < schedule.end_minute;
    }
    if (minute >= schedule.start_minute) return runsOn(day);
    if (minute < schedule.end_minute) return runsOn((day + 6) % 7);
    return false;
}

/**
 * The Schedules subview of Limits: every downtime schedule (on/off, edit,
 * delete) and the apps that stay open during them.
 */
export function DowntimeSection({ catalog, notify, guarded }: DowntimeSectionProps) {
    const { t } = useTranslation();
    const { schedules, allowlist, loading, createSchedule, updateSchedule, toggleSchedule, deleteSchedule, setAllowlist } =
        useDowntime(notify, guarded);

    const [editing, setEditing] = useState<ScheduleDto | "new" | null>(null);
    const [confirmDelete, setConfirmDelete] = useState<number | null>(null);
    const [selectedAppId, setSelectedAppId] = useState("");
    const now = new Date();

    // A delete asks once more, inline; the question goes away after a while.
    useEffect(() => {
        if (confirmDelete === null) return;
        const timer = window.setTimeout(() => setConfirmDelete(null), 5000);
        return () => window.clearTimeout(timer);
    }, [confirmDelete]);

    const availableApps = useMemo(() => {
        const allowed = new Set(allowlist.filter((i) => i.subject_type === "app").map((i) => i.subject_id));
        return (catalog?.apps ?? [])
            .filter((a) => !allowed.has(a.id))
            .sort((a, b) => a.display_name.localeCompare(b.display_name));
    }, [catalog, allowlist]);

    const allowApp = async (e: FormEvent) => {
        e.preventDefault();
        if (selectedAppId === "") return;
        const id = Number(selectedAppId);
        try {
            await setAllowlist("app", id, true, catalog?.apps.find((a) => a.id === id)?.display_name);
            setSelectedAppId("");
            notify("success", t("downtime.allowAdded"));
        } catch {
            // Reported by the hook.
        }
    };

    const removeAllowed = async (subjectType: string, subjectId: number) => {
        try {
            await setAllowlist(subjectType, subjectId, false);
            notify("success", t("downtime.allowRemoved"));
        } catch {
            // Reported by the hook.
        }
    };

    return (
        <>
            <div className="tt-head">
                <div>
                    <h1 className="tt-title">{t("limitsPage.schedules")}</h1>
                    <p className="tt-sub">{t("limitsPage.schedulesHint")}</p>
                </div>
            </div>

            <section className="tt-card" aria-labelledby="sched-list">
                <div className="tt-card-head">
                    <h2 id="sched-list" className="tt-card-title">{t("downtime.listTitle")}</h2>
                    <button type="button" className="tt-btn tt-btn--primary tt-btn--sm" onClick={() => setEditing("new")}>
                        <Plus size={16} aria-hidden="true" />
                        {t("downtime.newSchedule")}
                    </button>
                </div>
                {loading && schedules.length === 0 ? (
                    <LoadingSpinner size="md" />
                ) : schedules.length === 0 ? (
                    <p className="tt-sub">{t("downtime.noSchedules")}</p>
                ) : (
                    <ul className="tt-list">
                        {schedules.map((s) => {
                            const onNow = isOnNow(s, now);
                            const overnight = s.start_minute > s.end_minute;
                            return (
                                <li className="tt-row" key={s.id}>
                                    <span className="tt-row-text">
                                        <span className="tt-row-title">
                                            {s.name}
                                            {onNow && <span className="tt-tag tt-tag--on">{t("downtime.onNow")}</span>}
                                        </span>
                                        <span className="tt-row-detail">
                                            {clockLabel(s.start_minute)} – {clockLabel(s.end_minute)}
                                            {overnight ? ` (${t("downtime.overnight")})` : ""} · {daysLabel(s.weekday_mask)}
                                        </span>
                                    </span>
                                    <span className="tt-row-end">
                                        <input
                                            type="checkbox"
                                            role="switch"
                                            className="toggle-switch"
                                            checked={s.enabled}
                                            aria-label={t("downtime.toggleLabel", { name: s.name })}
                                            onChange={(e) => void toggleSchedule(s.id, e.target.checked)}
                                        />
                                        <button
                                            type="button"
                                            className="tt-btn tt-btn--outline tt-btn--sm"
                                            onClick={() => setEditing(s)}
                                            aria-label={t("downtime.editNamed", { name: s.name })}
                                        >
                                            <Pencil size={14} aria-hidden="true" />
                                            {t("limitsPage.edit")}
                                        </button>
                                        {confirmDelete === s.id ? (
                                            <button
                                                type="button"
                                                className="tt-btn tt-btn--danger tt-btn--sm"
                                                onClick={() => {
                                                    setConfirmDelete(null);
                                                    void deleteSchedule(s.id);
                                                }}
                                            >
                                                {t("downtime.confirmDelete")}
                                            </button>
                                        ) : (
                                            <button
                                                type="button"
                                                className="tt-icon-btn"
                                                onClick={() => setConfirmDelete(s.id)}
                                                aria-label={t("downtime.deleteNamed", { name: s.name })}
                                                title={t("downtime.deleteSchedule")}
                                            >
                                                <Trash2 size={16} aria-hidden="true" />
                                            </button>
                                        )}
                                    </span>
                                </li>
                            );
                        })}
                    </ul>
                )}
            </section>

            <section className="tt-card" aria-labelledby="sched-allowed">
                <h2 id="sched-allowed" className="tt-card-title">{t("downtime.allowlistTitle")}</h2>
                <p className="tt-sub">{t("downtime.allowlistDesc")}</p>
                <form className="tt-inline-form" onSubmit={(e) => void allowApp(e)}>
                    <label htmlFor="sched-allow-app" className="tt-sr-only">{t("downtime.searchAppToAllow")}</label>
                    <select
                        id="sched-allow-app"
                        className="tt-input tt-select"
                        value={selectedAppId}
                        onChange={(e) => setSelectedAppId(e.target.value)}
                    >
                        <option value="">{t("downtime.searchAppToAllow")}</option>
                        {availableApps.map((app) => (
                            <option key={app.id} value={app.id}>
                                {app.display_name}
                            </option>
                        ))}
                    </select>
                    <button type="submit" className="tt-btn tt-btn--outline tt-btn--sm" disabled={selectedAppId === ""}>
                        {t("downtime.addToAllowlist")}
                    </button>
                </form>
                {allowlist.length === 0 ? (
                    <p className="tt-sub">{t("downtime.noAllowlist")}</p>
                ) : (
                    <ul className="tt-chips">
                        {allowlist.map((item) => (
                            <li key={`${item.subject_type}-${item.subject_id}`} className="tt-chip tt-chip--removable">
                                {item.name}
                                <button
                                    type="button"
                                    onClick={() => void removeAllowed(item.subject_type, item.subject_id)}
                                    aria-label={t("downtime.allowRemoveNamed", { name: item.name })}
                                    title={t("downtime.allowRemove")}
                                >
                                    <X size={14} aria-hidden="true" />
                                </button>
                            </li>
                        ))}
                    </ul>
                )}
            </section>

            {editing && (
                <ScheduleEditor
                    schedule={editing === "new" ? null : editing}
                    onSave={async (name, mask, start, end) => {
                        if (editing === "new") await createSchedule(name, mask, start, end);
                        else await updateSchedule(editing.id, name, mask, start, end);
                        setEditing(null);
                    }}
                    onClose={() => setEditing(null)}
                />
            )}
        </>
    );
}

interface ScheduleEditorProps {
    schedule: ScheduleDto | null;
    onSave: (name: string, weekdayMask: number, startMinute: number, endMinute: number) => Promise<void>;
    onClose: () => void;
}

function ScheduleEditor({ schedule, onSave, onClose }: ScheduleEditorProps) {
    const { t } = useTranslation();
    const [name, setName] = useState(schedule?.name ?? t("setup.bedtimeName"));
    const [startTime, setStartTime] = useState(minuteToTimeString(schedule?.start_minute ?? 22 * 60));
    const [endTime, setEndTime] = useState(minuteToTimeString(schedule?.end_minute ?? 7 * 60));
    const [mask, setMask] = useState(schedule?.weekday_mask ?? EVERY_DAY);
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const names = weekdayShortNames();

    const start = timeStringToMinute(startTime);
    const end = timeStringToMinute(endTime);
    const title = schedule ? t("downtime.editSchedule") : t("downtime.newSchedule");

    const save = async (e: FormEvent) => {
        e.preventDefault();
        if (!name.trim()) return setError(t("downtime.errName"));
        if (mask === 0) return setError(t("downtime.errDays"));
        try {
            setSaving(true);
            setError(null);
            await onSave(name.trim(), mask, start, end);
        } catch (err) {
            setError(err instanceof Error && err.message ? err.message : t("downtime.errSave"));
        } finally {
            setSaving(false);
        }
    };

    const close = () => {
        if (!saving) onClose();
    };

    return (
        <Dialog label={title} onClose={close}>
            <form onSubmit={(e) => void save(e)}>
                <p className="dialog-eyebrow">{t("downtime.eyebrow")}</p>
                <h2 className="dialog-title">{title}</h2>

                <label className="field">
                    <span className="field-label">{t("downtime.scheduleName")}</span>
                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder={t("downtime.namePlaceholder")}
                        maxLength={60}
                    />
                </label>

                <div className="tt-field-pair">
                    <label className="field">
                        <span className="field-label">{t("downtime.startTime")}</span>
                        <input type="time" value={startTime} onChange={(e) => setStartTime(e.target.value)} required />
                    </label>
                    <label className="field">
                        <span className="field-label">{t("downtime.endTime")}</span>
                        <input type="time" value={endTime} onChange={(e) => setEndTime(e.target.value)} required />
                    </label>
                </div>
                {start > end && <p className="tt-note tt-note--calm">{t("downtime.overnightUntil", { time: clockLabel(end) })}</p>}

                <fieldset className="field tt-fieldset">
                    <legend className="field-label">{t("downtime.activeDays")}</legend>
                    <span className="tt-segmented" role="group" aria-label={t("downtime.quickDays")}>
                        <button type="button" aria-pressed={mask === EVERY_DAY} onClick={() => setMask(EVERY_DAY)}>
                            {t("downtime.everyday")}
                        </button>
                        <button type="button" aria-pressed={mask === WEEKDAYS} onClick={() => setMask(WEEKDAYS)}>
                            {t("downtime.weekdays")}
                        </button>
                        <button type="button" aria-pressed={mask === WEEKENDS} onClick={() => setMask(WEEKENDS)}>
                            {t("downtime.weekends")}
                        </button>
                    </span>
                    <span className="tt-day-pills">
                        {names.map((label, i) => (
                            <button
                                key={label}
                                type="button"
                                className="tt-day-pill"
                                aria-pressed={(mask & (1 << i)) !== 0}
                                onClick={() => setMask((m) => m ^ (1 << i))}
                            >
                                {label}
                            </button>
                        ))}
                    </span>
                </fieldset>

                {error && <p className="dialog-error">{error}</p>}

                <div className="dialog-actions">
                    <button type="button" className="btn btn--secondary" onClick={close} disabled={saving}>
                        {t("common.cancel")}
                    </button>
                    <button type="submit" className="btn btn--primary" disabled={saving}>
                        {saving && <LoadingSpinner size="xs" />}
                        {t("downtime.save")}
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
