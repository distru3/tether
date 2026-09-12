import { useMemo, useState, useRef, useEffect, type FormEvent, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { WeekdayMinutes } from "../api";
import { targetLabel, WEEKDAY_SHORT } from "../format";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { Dialog } from "./Dialog";
import { ChevronDownIcon, ChevronRightIcon } from "./icons/Icons";
import { LoadingSpinner } from "./LoadingSpinner";

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

interface PickerOption {
    value: string;
    label: string;
    group: "special" | "category" | "app";
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

// ---------------------------------------------------------------------------
// Custom searchable app picker - replaces the native <select> whose OS popup
// renders white and ignores all CSS.
// ---------------------------------------------------------------------------
interface AppPickerProps {
    options: PickerOption[];
    value: string;
    disabled: boolean;
    placeholder: string;
    onChange: (value: string) => void;
}

function AppPicker({ options, value, disabled, placeholder, onChange }: AppPickerProps) {
    const selectedLabel = options.find((o) => o.value === value)?.label ?? "";
    const [open, setOpen] = useState(false);
    const [query, setQuery] = useState("");
    const [activeIdx, setActiveIdx] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLUListElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    const filtered = useMemo(() => {
        const q = query.trim().toLowerCase();
        if (!q) return options;
        return options.filter((o) => o.label.toLowerCase().includes(q));
    }, [options, query]);

    // Reset active index when filter changes
    useEffect(() => { setActiveIdx(0); }, [query]);

    // Scroll active item into view
    useEffect(() => {
        if (!open) return;
        const el = listRef.current?.children[activeIdx] as HTMLElement | undefined;
        el?.scrollIntoView({ block: "nearest" });
    }, [activeIdx, open]);

    // Close on outside click
    useEffect(() => {
        if (!open) return;
        const handler = (e: MouseEvent) => {
            if (!containerRef.current?.contains(e.target as Node)) {
                setOpen(false);
                setQuery("");
            }
        };
        document.addEventListener("mousedown", handler);
        return () => document.removeEventListener("mousedown", handler);
    }, [open]);

    const openPicker = () => {
        if (disabled) return;
        setOpen(true);
        setQuery("");
        setActiveIdx(0);
        requestAnimationFrame(() => inputRef.current?.focus());
    };

    const select = (opt: PickerOption) => {
        onChange(opt.value);
        setOpen(false);
        setQuery("");
    };

    const onKeyDown = (e: KeyboardEvent) => {
        if (e.key === "ArrowDown") {
            e.preventDefault();
            setActiveIdx((i) => Math.min(i + 1, filtered.length - 1));
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActiveIdx((i) => Math.max(i - 1, 0));
        } else if (e.key === "Enter") {
            e.preventDefault();
            const opt = filtered[activeIdx];
            if (opt) select(opt);
        } else if (e.key === "Escape") {
            setOpen(false);
            setQuery("");
        }
    };

    return (
        <div ref={containerRef} className="app-picker" style={{ position: "relative" }}>
            {/* Trigger button */}
            <button
                type="button"
                className="app-picker__trigger"
                disabled={disabled}
                onClick={openPicker}
                aria-haspopup="listbox"
                aria-expanded={open}
            >
                <span className={value ? "app-picker__label" : "app-picker__placeholder"}>
                    {value ? selectedLabel : placeholder}
                </span>
                <ChevronDownIcon size={14} />
            </button>

            {/* Dropdown */}
            {open && (
                <div className="app-picker__dropdown">
                    <div className="app-picker__search-wrap">
                        <input
                            ref={inputRef}
                            className="app-picker__search"
                            type="text"
                            placeholder="Search..."
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            onKeyDown={onKeyDown}
                        />
                    </div>
                    {filtered.length === 0 ? (
                        <p className="app-picker__empty">No matches</p>
                    ) : (
                        <ul ref={listRef} className="app-picker__list" role="listbox">
                            {filtered.map((opt, i) => (
                                <li
                                    key={opt.value}
                                    role="option"
                                    aria-selected={opt.value === value}
                                    className={[
                                        "app-picker__item",
                                        opt.value === value ? "app-picker__item--selected" : "",
                                        i === activeIdx ? "app-picker__item--active" : "",
                                        opt.group === "special" ? "app-picker__item--special" : "",
                                        opt.group === "category" ? "app-picker__item--category" : "",
                                    ].filter(Boolean).join(" ")}
                                    onMouseEnter={() => setActiveIdx(i)}
                                    onMouseDown={(e) => { e.preventDefault(); select(opt); }}
                                >
                                    {opt.label}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}
        </div>
    );
}

export function LimitEditorDialog({ catalog, target, limit, busy, onSubmit, onClose }: LimitEditorDialogProps) {
    const { t } = useTranslation();
    const limitableCategories = useMemo(
        () => (catalog?.categories ?? []).filter((category) => category.kind === "limitable"),
        [catalog],
    );

    // Build flat option list for the picker
    const pickerOptions = useMemo((): PickerOption[] => {
        const opts: PickerOption[] = [];
        if (!catalog?.limits.some((l) => l.target.kind === "total")) {
            opts.push({ value: "total", label: t("limitEditor.totalScreenTime"), group: "special" });
        }
        for (const cat of limitableCategories) {
            if (!catalog?.limits.some((l) => l.target.kind === "category" && l.target.id === cat.id)) {
                opts.push({ value: `category:${cat.id}`, label: cat.name, group: "category" });
            }
        }
        for (const app of (catalog?.apps ?? [])) {
            if (!catalog?.limits.some((l) => l.target.kind === "app" && l.target.id === app.id)) {
                opts.push({ value: `app:${app.id}`, label: app.display_name, group: "app" });
            }
        }
        return opts;
    }, [catalog, limitableCategories, t]);

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
                        <AppPicker
                            options={pickerOptions}
                            value={chosen}
                            disabled={busy}
                            placeholder={t("limitEditor.choose")}
                            onChange={(v) => { setChosen(v); setError(null); }}
                        />
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
                    {overrideCount > 0 && <span className="weekday-count">({overrideCount} active)</span>}
                </button>
                {weekOpen && (
                    <div className="weekday-grid">
                        {dayOverrides.map((day, index) => (
                            <div
                                key={WEEKDAY_SHORT[index]}
                                className={`weekday-row ${day.override ? "weekday-row--active" : ""}`}
                            >
                                <label className="weekday-label">
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
                                    <span className="weekday-name">
                                        {t(`weeklyChart.${WEEKDAY_SHORT[index]!.toLowerCase()}`)}
                                    </span>
                                </label>
                                <div className="weekday-input-wrap">
                                    <input
                                        type="number"
                                        className="weekday-input"
                                        min={0}
                                        max={1440}
                                        step={1}
                                        placeholder={minutes ? `${minutes}` : "60"}
                                        value={day.minutes}
                                        disabled={busy || !day.override}
                                        aria-label={`${WEEKDAY_SHORT[index]} minutes`}
                                        onChange={(event) => updateDay(index, { minutes: event.target.value })}
                                    />
                                    <span className="weekday-unit">min</span>
                                </div>
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
                        {busy && <LoadingSpinner size="xs" />}
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

