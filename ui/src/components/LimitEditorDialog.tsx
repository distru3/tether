import { useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { WeekdayMinutes } from "../api";
import { targetLabel, WEEKDAY_SHORT } from "../format";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { Dialog } from "./Dialog";
import { ChevronDownIcon, ChevronRightIcon } from "./icons/Icons";

interface LimitEditorDialogProps {
    catalog: CatalogDto | null;
    target: LimitTargetDto | null;
    limit: LimitDto | null;
    busy: boolean;
    onSubmit: (target: LimitTargetDto, minutes: number, weekdayMinutes: WeekdayMinutes, enabled: boolean) => void;
    onClose: () => void;
}

interface DayOverrideState {
    override: boolean;
    minutes: string;
}

function initDayOverrides(limit: LimitDto | null): DayOverrideState[] {
    return WEEKDAY_SHORT.map((_, day) => {
        const value = limit?.weekday_minutes[day] ?? null;
        return value === null ? { override: false, minutes: "" } : { override: true, minutes: String(value) };
    });
}

function encodeTarget(target: LimitTargetDto | null): string {
    if (target === null) return "";
    return target.kind === "total" ? "total" : `${target.kind}:${target.id}`;
}

function decodeTarget(value: string): LimitTargetDto | null {
    if (value === "total") return { kind: "total" };
    const [kind, rawId] = value.split(":");
    const id = Number.parseInt(rawId ?? "", 10);
    if ((kind === "app" || kind === "category") && Number.isFinite(id)) return { kind, id };
    return null;
}

export function LimitEditorDialog({ catalog, target, limit, busy, onSubmit, onClose }: LimitEditorDialogProps) {
    const { t } = useTranslation();
    const limitableCategories = useMemo(
        () => (catalog?.categories ?? []).filter((category) => category.kind === "limitable"),
        [catalog],
    );
    const [chosen, setChosen] = useState(() => encodeTarget(target));
    const [minutes, setMinutes] = useState(() => String(limit?.default_minutes ?? 60));
    const [enabled, setEnabled] = useState(() => limit?.enabled ?? true);
    const [dayOverrides, setDayOverrides] = useState<DayOverrideState[]>(() => initDayOverrides(limit));
    const [weekOpen, setWeekOpen] = useState(() => (limit?.weekday_minutes ?? []).some((m) => m !== null));
    const [error, setError] = useState<string | null>(null);

    const locked = target !== null;
    const overrideCount = dayOverrides.filter((day) => day.override).length;

    const updateDay = (day: number, patch: Partial<DayOverrideState>) => {
        setDayOverrides((prev) => prev.map((entry, i) => (i === day ? { ...entry, ...patch } : entry)));
        setError(null);
    };

    const submit = (event: FormEvent) => {
        event.preventDefault();
        if (busy) return;
        const parsed = Number.parseInt(minutes, 10);
        if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1440) {
            setError(t("limitEditor.errMinutes"));
            return;
        }
        for (const day of dayOverrides) {
            if (!day.override) continue;
            const dayMinutes = Number.parseInt(day.minutes, 10);
            if (!Number.isFinite(dayMinutes) || dayMinutes < 0 || dayMinutes > 1440) {
                setError(t("limitEditor.errOverrideMinutes"));
                setWeekOpen(true);
                return;
            }
        }
        const weekdayMinutes = dayOverrides.map((day) =>
            day.override ? Number.parseInt(day.minutes, 10) : null,
        ) as WeekdayMinutes;
        const finalTarget = locked ? target : decodeTarget(chosen);
        if (finalTarget === null) {
            setError(t("limitEditor.errPick"));
            return;
        }
        onSubmit(finalTarget, parsed, weekdayMinutes, enabled);
    };

    return (
        <Dialog
            label={locked ? t("limitEditor.editOrder") : t("limitEditor.newOrder")}
            onClose={() => {
                if (!busy) onClose();
            }}
        >
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">{locked ? t("limitEditor.editEyebrow") : t("limitEditor.newEyebrow")}</p>
                <h2 className="dialog-title">
                    {locked && target !== null ? targetLabel(target, catalog) : t("limitEditor.chooseTarget")}
                </h2>
                {!locked && (
                    <label className="field">
                        <span className="field-label">{t("limitEditor.appliesTo")}</span>
                        <select
                            value={chosen}
                            disabled={busy}
                            onChange={(event) => {
                                setChosen(event.target.value);
                                setError(null);
                            }}
                        >
                            <option value="" disabled>
                                {t("limitEditor.choose")}
                            </option>
                            {!catalog?.limits.some((l) => l.target.kind === "total") && (
                                <option value="total">{t("limitEditor.totalScreenTime")}</option>
                            )}
                            {limitableCategories
                                .filter((category) => !catalog?.limits.some((l) => l.target.kind === "category" && l.target.id === category.id))
                                .map((category) => (
                                    <option key={`category:${category.id}`} value={`category:${category.id}`}>
                                        {category.name}
                                    </option>
                                ))}
                            {(catalog?.apps ?? [])
                                .filter((app) => !catalog?.limits.some((l) => l.target.kind === "app" && l.target.id === app.id))
                                .map((app) => (
                                    <option key={`app:${app.id}`} value={`app:${app.id}`}>
                                        {app.display_name}
                                    </option>
                                ))}
                        </select>
                    </label>
                )}
                <label className="field">
                    <span className="field-label">{t("limitEditor.minutesPerDay")}</span>
                    <input
                        type="number"
                        min={0}
                        max={1440}
                        step={1}
                        value={minutes}
                        disabled={busy}
                        onChange={(event) => {
                            setMinutes(event.target.value);
                            setError(null);
                        }}
                    />
                </label>
                <button
                    type="button"
                    className="weekday-toggle"
                    disabled={busy}
                    onClick={() => setWeekOpen((open) => !open)}
                >
                    <span className="weekday-chevron">{weekOpen ? <ChevronDownIcon size={14} /> : <ChevronRightIcon size={14} />}</span>
                    {t("limitEditor.perDayOverrides")}
                    {overrideCount > 0 && <span className="weekday-count">· {overrideCount}</span>}
                </button>
                {weekOpen && (
                    <div className="weekday-grid">
                        {dayOverrides.map((day, index) => (
                            <div key={WEEKDAY_SHORT[index]} className="weekday-row">
                                <input
                                    type="checkbox"
                                    className="check-input"
                                    checked={day.override}
                                    disabled={busy}
                                    aria-label={`Override ${WEEKDAY_SHORT[index]}`}
                                    onChange={(event) =>
                                        updateDay(index, { override: event.target.checked })
                                    }
                                />
                                <span className="weekday-name">{t(`weeklyChart.${WEEKDAY_SHORT[index]!.toLowerCase()}`)}</span>
                                <input
                                    type="number"
                                    className="weekday-input"
                                    min={0}
                                    max={1440}
                                    step={1}
                                    placeholder="—"
                                    value={day.minutes}
                                    disabled={busy || !day.override}
                                    aria-label={`${WEEKDAY_SHORT[index]} minutes`}
                                    onChange={(event) => updateDay(index, { minutes: event.target.value })}
                                />
                            </div>
                        ))}
                    </div>
                )}
                <label className="field field--inline">
                    <input
                        type="checkbox"
                        checked={enabled}
                        disabled={busy}
                        onChange={(event) => setEnabled(event.target.checked)}
                    />
                    <span>{t("limitEditor.inForce")}</span>
                </label>
                {error !== null && <p className="dialog-error">{error}</p>}
                <div className="dialog-actions">
                    <button type="submit" className="btn btn--primary" disabled={busy}>
                        {busy ? t("limitEditor.setting") : t("limitEditor.saveOrder")}
                    </button>
                    <button
                        type="button"
                        className="btn btn--secondary"
                        onClick={() => {
                            if (!busy) onClose();
                        }}
                        disabled={busy}
                    >
                        {t("common.cancel")}
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
