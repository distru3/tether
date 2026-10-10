import { useMemo, useState, useRef, useEffect, type CSSProperties, type FormEvent, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Smartphone, Layers, LayoutGrid, Clock, Calendar, Check } from "lucide-react";
import type { WeekdayMinutes } from "../api";
import { formatDuration, formatWhen, targetLabel, WEEKDAY_SHORT, weekdayShortNames } from "../format";
import { waitsForCooldown } from "../limitRules";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { Dialog } from "./Dialog";
import { ChevronDownIcon } from "./icons/Icons";
import { LoadingSpinner } from "./LoadingSpinner";

interface LimitEditorDialogProps {
    catalog: CatalogDto | null;
    target: LimitTargetDto | null;
    limit: LimitDto | null;
    busy: boolean;
    onSubmit: (target: LimitTargetDto, minutes: number, weekdayMinutes: WeekdayMinutes, enabled: boolean) => void;
    onClose: () => void;
    onCategorize?: (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => void;
    /** Hours a raised limit waits before it applies (status `limit_cooldown_hours`). */
    cooldownHours: number;
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

/** The duration slider's linear range (minutes) and its labelled ticks. */
const SLIDER_MIN = 5;
const SLIDER_MAX = 480;
// No 15m tick: next to "1h" the two labels collide.
const SLIDER_TICKS = [60, 120, 240, 360, SLIDER_MAX];

/** Compact per-day budget label, in the app-wide duration style. */
function formatShortDuration(totalMins: number): string {
    return formatDuration(totalMins * 60);
}

// ---------------------------------------------------------------------------
// Searchable App/Category Picker
// ---------------------------------------------------------------------------
interface AppPickerProps {
    options: PickerOption[];
    value: string;
    disabled: boolean;
    placeholder: string;
    onChange: (value: string) => void;
}

function AppPicker({ options, value, disabled, placeholder, onChange }: AppPickerProps) {
    const { t } = useTranslation();
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

    useEffect(() => { setActiveIdx(0); }, [query]);

    useEffect(() => {
        if (!open) return;
        const el = listRef.current?.children[activeIdx] as HTMLElement | undefined;
        el?.scrollIntoView({ block: "nearest" });
    }, [activeIdx, open]);

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
            if (filtered[activeIdx]) select(filtered[activeIdx]);
        } else if (e.key === "Escape") {
            setOpen(false);
            setQuery("");
        }
    };

    return (
        <div className="custom-app-picker" ref={containerRef}>
            <button
                type="button"
                className={`picker-trigger ${open ? "picker-trigger--open" : ""} ${disabled ? "picker-trigger--disabled" : ""}`}
                onClick={open ? () => setOpen(false) : openPicker}
                disabled={disabled}
                aria-haspopup="listbox"
                aria-expanded={open}
            >
                <span className={selectedLabel ? "picker-value" : "picker-placeholder"}>
                    {selectedLabel || placeholder}
                </span>
                <span className="picker-chevron">
                    <ChevronDownIcon size={14} />
                </span>
            </button>

            {open && (
                <div className="picker-popover">
                    <div className="picker-search-wrap">
                        <input
                            ref={inputRef}
                            type="text"
                            className="picker-search-input"
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            onKeyDown={onKeyDown}
                            placeholder={t("limitEditor.searchPlaceholder")}
                        />
                    </div>
                    <ul className="picker-list" ref={listRef} role="listbox">
                        {filtered.length === 0 ? (
                            <li className="picker-empty">{t("limitEditor.noMatches")}</li>
                        ) : (
                            filtered.map((opt, i) => (
                                <li
                                    key={opt.value}
                                    role="option"
                                    aria-selected={opt.value === value}
                                    className={`picker-item ${opt.value === value ? "picker-item--selected" : ""} ${i === activeIdx ? "picker-item--active" : ""}`}
                                    onMouseDown={(e) => {
                                        e.preventDefault();
                                        select(opt);
                                    }}
                                    onMouseEnter={() => setActiveIdx(i)}
                                >
                                    <span className="picker-item-label">{opt.label}</span>
                                    {opt.value === value && <Check size={14} className="picker-item-check" />}
                                </li>
                            ))
                        )}
                    </ul>
                </div>
            )}
        </div>
    );
}

// ---------------------------------------------------------------------------
// LimitEditorDialog Component
// ---------------------------------------------------------------------------
export function LimitEditorDialog({
    catalog,
    target,
    limit,
    busy,
    onSubmit,
    onClose,
    onCategorize,
    cooldownHours,
}: LimitEditorDialogProps) {
    const { t } = useTranslation();

    const limitableCategories = useMemo(
        () => (catalog?.categories ?? []).filter((cat) => cat.id > 0),
        [catalog],
    );

    const appOptions = useMemo((): PickerOption[] => {
        const opts: PickerOption[] = [];
        for (const app of (catalog?.apps ?? [])) {
            if (!catalog?.limits.some((l) => l.target.kind === "app" && l.target.id === app.id)) {
                const cat = catalog?.categories.find((c) => c.id === app.primary_category);
                const catSuffix = cat ? ` • ${cat.name}` : "";
                opts.push({ value: `app:${app.id}`, label: `${app.display_name}${catSuffix}`, group: "app" });
            }
        }
        return opts;
    }, [catalog]);

    const categoryOptions = useMemo((): PickerOption[] => {
        const opts: PickerOption[] = [];
        for (const cat of limitableCategories) {
            if (!catalog?.limits.some((l) => l.target.kind === "category" && l.target.id === cat.id)) {
                opts.push({ value: `category:${cat.id}`, label: cat.name, group: "category" });
            }
        }
        return opts;
    }, [catalog, limitableCategories]);

    const locked = target !== null;

    const [targetKind, setTargetKind] = useState<"app" | "category" | "total">(() => {
        if (target) return target.kind;
        return "app";
    });

    const [chosen, setChosen] = useState(() => {
        if (target) return encodeTarget(target);
        return appOptions[0]?.value ?? categoryOptions[0]?.value ?? "total";
    });

    const [minutes, setMinutes] = useState(() => String(limit?.default_minutes ?? 60));
    const [enabled, setEnabled] = useState(() => limit?.enabled ?? true);
    const [dayOverrides, setDayOverrides] = useState<DayOverrideState[]>(() => initDayOverrides(limit));
    const [focusedDay, setFocusedDay] = useState<number>(0);
    const [error, setError] = useState<string | null>(null);

    const overrideCount = dayOverrides.filter((day) => day.override).length;
    const currentTarget = locked ? target : decodeTarget(chosen);
    const currentApp = currentTarget?.kind === "app" ? catalog?.apps.find((a) => a.id === currentTarget.id) : null;
    const currentCat = currentApp ? catalog?.categories.find((c) => c.id === currentApp.primary_category) : null;
    const currentCatName = currentCat?.name ?? t("categorize.uncategorized");

    // Synchronize target kind switcher
    const handleSegmentChange = (kind: "app" | "category" | "total") => {
        setTargetKind(kind);
        setError(null);
        if (kind === "total") {
            setChosen("total");
        } else if (kind === "category") {
            if (!chosen.startsWith("category:") && categoryOptions[0]) {
                setChosen(categoryOptions[0].value);
            }
        } else {
            if (!chosen.startsWith("app:") && appOptions[0]) {
                setChosen(appOptions[0].value);
            }
        }
    };

    // Duration dual hours / minutes numeric input math
    const totalMinutesVal = Math.max(0, Math.min(1440, Number.parseInt(minutes, 10) || 0));
    const hoursVal = Math.floor(totalMinutesVal / 60);
    const minsVal = totalMinutesVal % 60;

    const setHours = (h: number) => {
        const clampedH = Math.max(0, Math.min(24, h));
        const newTotal = clampedH * 60 + minsVal;
        setMinutes(String(Math.min(1440, newTotal)));
        setError(null);
    };

    const setMins = (m: number) => {
        const clampedM = Math.max(0, Math.min(59, m));
        const newTotal = hoursVal * 60 + clampedM;
        setMinutes(String(Math.min(1440, newTotal)));
        setError(null);
    };

    const DURATION_PRESETS = [15, 30, 60, 120, 180, 240].map((val) => ({ label: formatShortDuration(val), val }));

    // Weekday schedule quick helpers
    const toggleAllDays = () => {
        const allActive = dayOverrides.every((d) => d.override);
        setDayOverrides((prev) =>
            prev.map((d) => ({
                override: !allActive,
                minutes: !allActive ? (d.minutes || minutes || "60") : "",
            }))
        );
        setError(null);
    };

    const toggleWeekdaysOnly = () => {
        const weekdaysActive = dayOverrides.slice(0, 5).every((d) => d.override);
        setDayOverrides((prev) =>
            prev.map((d, idx) => {
                if (idx < 5) {
                    return {
                        override: !weekdaysActive,
                        minutes: !weekdaysActive ? (d.minutes || minutes || "60") : "",
                    };
                }
                return d;
            })
        );
        setError(null);
    };

    const toggleWeekendsOnly = () => {
        const weekendsActive = dayOverrides.slice(5, 7).every((d) => d.override);
        setDayOverrides((prev) =>
            prev.map((d, idx) => {
                if (idx >= 5) {
                    return {
                        override: !weekendsActive,
                        minutes: !weekendsActive ? (d.minutes || minutes || "60") : "",
                    };
                }
                return d;
            })
        );
        setError(null);
    };

    const toggleSingleDay = (dayIdx: number) => {
        setDayOverrides((prev) =>
            prev.map((d, i) =>
                i === dayIdx
                    ? {
                          override: !d.override,
                          minutes: !d.override ? (d.minutes || minutes || "60") : "",
                      }
                    : d
            )
        );
        setError(null);
    };

    const updateDayMinutes = (dayIdx: number, val: string) => {
        setDayOverrides((prev) =>
            prev.map((d, i) => (i === dayIdx ? { ...d, minutes: val } : d))
        );
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

    // When saving would take effect (docs/DESIGN_SYSTEM.md §2.3): shown before
    // the save button, so the cooldown is a promise rather than a surprise.
    const parsedMinutes = Number.parseInt(minutes, 10);
    const draftWeekdays = dayOverrides.map((day) => (day.override ? Number.parseInt(day.minutes, 10) || 0 : null));
    const waits =
        cooldownHours > 0 &&
        Number.isFinite(parsedMinutes) &&
        waitsForCooldown(limit, parsedMinutes, draftWeekdays, enabled);
    const appliesAt = formatWhen(new Date(Date.now() + cooldownHours * 3600 * 1000));

    // Active focused day inspection math
    const activeFocusedDay = dayOverrides[focusedDay];
    const activeFocusedMins = Number.parseInt(activeFocusedDay?.minutes ?? "", 10) || 0;
    const activeFocusedH = Math.floor(activeFocusedMins / 60);
    const activeFocusedM = activeFocusedMins % 60;

    return (
        <Dialog
            label={locked ? t("limitEditor.editOrder") : t("limitEditor.newOrder")}
            onClose={() => {
                if (!busy) onClose();
            }}
        >
            <form onSubmit={submit} className="limit-editor-form">
                <div className="limit-editor-header">
                    <p className="dialog-eyebrow">
                        {locked ? t("limitEditor.editEyebrow") : t("limitEditor.newEyebrow")}
                    </p>
                    <h2 className="dialog-title">
                        {locked && target !== null ? targetLabel(target, catalog) : t("limitEditor.chooseTarget")}
                    </h2>
                </div>

                {/* 1. Target Picker Segmented Control */}
                {!locked && (
                    <div className="limit-form-section">
                        <div className="target-segmented-control">
                            <button
                                type="button"
                                className={`target-segment-btn ${targetKind === "app" ? "target-segment-btn--active" : ""}`}
                                onClick={() => handleSegmentChange("app")}
                            >
                                <Smartphone size={14} />
                                <span>{t("limits.app")}</span>
                            </button>
                            <button
                                type="button"
                                className={`target-segment-btn ${targetKind === "category" ? "target-segment-btn--active" : ""}`}
                                onClick={() => handleSegmentChange("category")}
                            >
                                <Layers size={14} />
                                <span>{t("limits.category")}</span>
                            </button>
                            <button
                                type="button"
                                className={`target-segment-btn ${targetKind === "total" ? "target-segment-btn--active" : ""}`}
                                onClick={() => handleSegmentChange("total")}
                            >
                                <LayoutGrid size={14} />
                                <span>{t("limitEditor.totalScreenTime")}</span>
                            </button>
                        </div>

                        {targetKind === "app" && (
                            <AppPicker
                                options={appOptions}
                                value={chosen}
                                disabled={busy}
                                placeholder={t("limitEditor.choose")}
                                onChange={(v) => { setChosen(v); setError(null); }}
                            />
                        )}

                        {targetKind === "category" && (
                            <AppPicker
                                options={categoryOptions}
                                value={chosen}
                                disabled={busy}
                                placeholder={t("limitEditor.selectCategory")}
                                onChange={(v) => { setChosen(v); setError(null); }}
                            />
                        )}

                        {targetKind === "total" && (
                            <div className="total-device-callout">
                                <LayoutGrid size={18} className="total-device-icon" />
                                <div className="total-device-text">
                                    <strong>{t("limitEditor.totalScreenTime")}:</strong> {t("limitEditor.totalHint")}
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {/* Primary Category Tag for Selected App */}
                {currentApp && (
                    <div className="limit-app-category-bar">
                        <div className="limit-app-category-info">
                            <span className="limit-app-category-label">{t("categorize.primary")}</span>
                            <span className="tt-tag">{currentCatName}</span>
                        </div>
                        {onCategorize && (
                            <button
                                type="button"
                                className="tt-link"
                                onClick={() => onCategorize(currentApp.id, currentApp.display_name, currentApp.primary_category, currentApp.tags)}
                            >
                                {t("categorize.changeCategory")}
                            </button>
                        )}
                    </div>
                )}

                {/* 2. Duration Controls (Dual Numeric Inputs, Presets, and Slider) */}
                <div className="limit-form-section">
                    <div className="limit-section-header">
                        <div className="limit-section-title">
                            <Clock size={13} className="limit-section-icon" />
                            <span>{t("limitEditor.minutesPerDay")}</span>
                        </div>
                        <div className="duration-dual-inputs">
                            <div className="duration-input-group">
                                <input
                                    type="number"
                                    className="duration-num-input"
                                    aria-label={t("limitEditor.hours")}
                                    min={0}
                                    max={24}
                                    value={hoursVal}
                                    disabled={busy}
                                    onChange={(e) => setHours(Number.parseInt(e.target.value, 10) || 0)}
                                />
                                <span className="duration-unit">{t("time.short.h")}</span>
                            </div>
                            <span className="duration-separator">:</span>
                            <div className="duration-input-group">
                                <input
                                    type="number"
                                    className="duration-num-input"
                                    aria-label={t("limitEditor.minutes")}
                                    min={0}
                                    max={59}
                                    value={minsVal}
                                    disabled={busy}
                                    onChange={(e) => setMins(Number.parseInt(e.target.value, 10) || 0)}
                                />
                                <span className="duration-unit">{t("time.short.m")}</span>
                            </div>
                        </div>
                    </div>

                    {/* Quick Preset Pills */}
                    <div className="duration-presets">
                        {DURATION_PRESETS.map((preset) => (
                            <button
                                key={preset.val}
                                type="button"
                                className={`preset-pill ${totalMinutesVal === preset.val ? "preset-pill--active" : ""}`}
                                onClick={() => { setMinutes(String(preset.val)); setError(null); }}
                            >
                                {preset.label}
                            </button>
                        ))}
                    </div>

                    {/* Smooth Duration Slider */}
                    <div className="duration-slider-wrap">
                        <input
                            type="range"
                            className="duration-slider"
                            aria-label={t("limitEditor.minutesPerDay")}
                            aria-valuetext={formatShortDuration(Math.min(totalMinutesVal, SLIDER_MAX))}
                            min={SLIDER_MIN}
                            max={SLIDER_MAX}
                            step={5}
                            value={Math.min(totalMinutesVal, SLIDER_MAX)}
                            disabled={busy}
                            onChange={(e) => { setMinutes(e.target.value); setError(null); }}
                        />
                        <div className="duration-slider-labels" aria-hidden="true">
                            {SLIDER_TICKS.map((mins) => (
                                <span key={mins} style={{ "--at": (mins - SLIDER_MIN) / (SLIDER_MAX - SLIDER_MIN) } as CSSProperties}>
                                    {formatShortDuration(mins)}{mins === SLIDER_MAX ? "+" : ""}
                                </span>
                            ))}
                        </div>
                    </div>
                </div>

                {/* 3. Interactive Weekday Schedule Selector */}
                <div className="limit-form-section">
                    <div className="limit-section-header">
                        <div className="limit-section-title">
                            <Calendar size={13} className="limit-section-icon" />
                            <span>{t("limitEditor.perDayOverrides")}</span>
                            {overrideCount > 0 && (
                                <span className="weekday-active-count-badge">
                                    {t("limitEditor.customCount", { count: overrideCount })}
                                </span>
                            )}
                        </div>
                        <div className="weekday-helpers">
                            <button type="button" className="weekday-helper-btn" onClick={toggleAllDays}>{t("limitEditor.everyDay")}</button>
                            <button type="button" className="weekday-helper-btn" onClick={toggleWeekdaysOnly}>{t("limitEditor.weekdays")}</button>
                            <button type="button" className="weekday-helper-btn" onClick={toggleWeekendsOnly}>{t("limitEditor.weekends")}</button>
                        </div>
                    </div>

                    {/* 7-Cell Schedule Matrix */}
                    <div className="weekday-pill-bar">
                        {weekdayShortNames().map((dayName, idx) => {
                            const isOverridden = dayOverrides[idx]?.override;
                            const isFocused = focusedDay === idx;
                            const dayMins = Number.parseInt(dayOverrides[idx]?.minutes || "0", 10);
                            return (
                                <button
                                    key={dayName}
                                    type="button"
                                    className={`weekday-day-cell ${isOverridden ? "weekday-day-cell--active" : ""} ${isFocused ? "weekday-day-cell--focused" : ""}`}
                                    onClick={() => {
                                        setFocusedDay(idx);
                                        if (!isOverridden) {
                                            toggleSingleDay(idx);
                                        }
                                    }}
                                    title={`${dayName}: ${isOverridden ? formatShortDuration(dayMins) : t("limitEditor.dailyDefault")}`}
                                >
                                    <span className="day-cell-name">{dayName}</span>
                                    <span className="day-cell-time">
                                        {isOverridden ? formatShortDuration(dayMins) : t("limitEditor.daily")}
                                    </span>
                                </button>
                            );
                        })}
                    </div>

                    {/* Focused Day Inline Adjuster */}
                    <div className="weekday-focus-editor">
                        <div className="weekday-focus-info">
                            <span className="weekday-focus-day">{weekdayShortNames()[focusedDay]}</span>
                            <span className="weekday-focus-status">
                                {activeFocusedDay?.override 
                                    ? t("limitEditor.customOverride") 
                                    : t("limitEditor.usingDaily")}
                            </span>
                        </div>

                        {activeFocusedDay?.override ? (
                            <div className="weekday-focus-controls">
                                <button
                                    type="button"
                                    className="weekday-step-btn"
                                    onClick={() => {
                                        const next = Math.max(0, activeFocusedMins - 15);
                                        updateDayMinutes(focusedDay, String(next));
                                    }}
                                    title={t("limitEditor.minus15")}
                                    aria-label={t("limitEditor.minus15")}
                                >
                                    −{formatShortDuration(15)}
                                </button>

                                <div className="duration-dual-inputs duration-dual-inputs--sm">
                                    <div className="duration-input-group">
                                        <input
                                            type="number"
                                            className="duration-num-input duration-num-input--sm"
                                            aria-label={t("limitEditor.hours")}
                                            min={0}
                                            max={24}
                                            value={activeFocusedH}
                                            onChange={(e) => {
                                                const h = Math.max(0, Math.min(24, Number.parseInt(e.target.value, 10) || 0));
                                                updateDayMinutes(focusedDay, String(h * 60 + activeFocusedM));
                                            }}
                                        />
                                        <span className="duration-unit">{t("time.short.h")}</span>
                                    </div>
                                    <span className="duration-separator">:</span>
                                    <div className="duration-input-group">
                                        <input
                                            type="number"
                                            className="duration-num-input duration-num-input--sm"
                                            aria-label={t("limitEditor.minutes")}
                                            min={0}
                                            max={59}
                                            value={activeFocusedM}
                                            onChange={(e) => {
                                                const m = Math.max(0, Math.min(59, Number.parseInt(e.target.value, 10) || 0));
                                                updateDayMinutes(focusedDay, String(activeFocusedH * 60 + m));
                                            }}
                                        />
                                        <span className="duration-unit">{t("time.short.m")}</span>
                                    </div>
                                </div>

                                <button
                                    type="button"
                                    className="weekday-step-btn"
                                    onClick={() => {
                                        const next = Math.min(1440, activeFocusedMins + 15);
                                        updateDayMinutes(focusedDay, String(next));
                                    }}
                                    title={t("limitEditor.plus15")}
                                    aria-label={t("limitEditor.plus15")}
                                >
                                    +{formatShortDuration(15)}
                                </button>

                                <button
                                    type="button"
                                    className="weekday-action-btn weekday-action-btn--reset"
                                    onClick={() => toggleSingleDay(focusedDay)}
                                    title={t("limitEditor.resetTitle")}
                                >
                                    {t("limitEditor.removeOverride")}
                                </button>

                                {overrideCount > 1 && (
                                    <button
                                        type="button"
                                        className="weekday-action-btn weekday-action-btn--sync"
                                        onClick={() => {
                                            const targetMins = activeFocusedDay.minutes;
                                            setDayOverrides((prev) =>
                                                prev.map((d) => (d.override ? { ...d, minutes: targetMins } : d))
                                            );
                                        }}
                                        title={t("limitEditor.syncTitle")}
                                    >
                                        {t("limitEditor.syncAll")}
                                    </button>
                                )}
                            </div>
                        ) : (
                            <div className="weekday-focus-inactive">
                                <button
                                    type="button"
                                    className="btn btn-secondary btn-xs"
                                    onClick={() => toggleSingleDay(focusedDay)}
                                >
                                    {t("limitEditor.enableForDay", "Customize {{day}}", { day: weekdayShortNames()[focusedDay] })}
                                </button>
                            </div>
                        )}
                    </div>
                </div>

                {/* 4. In Force Checkbox */}
                <label className="limit-enforce-toggle">
                    <span className="limit-enforce-label">{t("limitEditor.inForce")}</span>
                    <input
                        type="checkbox"
                        role="switch"
                        className="toggle-switch"
                        checked={enabled}
                        disabled={busy}
                        onChange={(event) => setEnabled(event.target.checked)}
                    />
                </label>

                {waits ? (
                    <div className="tt-note" role="note">
                        <span>
                            <Clock size={18} className="tt-note-icon" aria-hidden="true" />
                            <span>
                                <strong>{t("limitEditor.appliesAt", { when: appliesAt })}</strong>
                                {t("limitEditor.appliesAtWhy", { count: cooldownHours })}
                            </span>
                        </span>
                    </div>
                ) : (
                    <div className="tt-note tt-note--calm" role="note">
                        <span>
                            <Clock size={18} className="tt-note-icon" aria-hidden="true" />
                            <span>{t("limitEditor.appliesNow")}</span>
                        </span>
                    </div>
                )}

                {error !== null && (
                    <div className="dialog-error">
                        {error}
                    </div>
                )}

                {/* 5. Dialog Actions */}
                <div className="dialog-actions">
                    <button
                        type="button"
                        className="btn btn--secondary"
                        onClick={() => { if (!busy) onClose(); }}
                        disabled={busy}
                    >
                        {t("common.cancel")}
                    </button>
                    <button type="submit" className="btn btn--primary" disabled={busy}>
                        {busy && <LoadingSpinner size="xs" />}
                        {busy
                            ? t("limitEditor.setting")
                            : waits
                              ? t("limitEditor.saveLater")
                              : t("limitEditor.saveOrder")}
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
