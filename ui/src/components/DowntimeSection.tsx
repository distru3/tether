import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
    Moon,
    Clock,
    Plus,
    Trash2,
    Edit2,
    ShieldCheck,
    Check,
    AlertCircle,
    Calendar,
    Sparkles,
    X,
    Search,
} from "lucide-react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import { useDowntime } from "../hooks/useDowntime";
import { Dialog } from "./Dialog";
import { ToggleSwitch } from "./ToggleSwitch";
import { MetricCards, type MetricData } from "./MetricCards";
import { LoadingSpinner } from "./LoadingSpinner";

interface DowntimeSectionProps {
    catalog: CatalogDto | null;
    notify: (kind: "success" | "error", message: string) => void;
}

const WEEKDAYS = [
    { label: "M", full: "Mon", bit: 1 << 0 },
    { label: "T", full: "Tue", bit: 1 << 1 },
    { label: "W", full: "Wed", bit: 1 << 2 },
    { label: "T", full: "Thu", bit: 1 << 3 },
    { label: "F", full: "Fri", bit: 1 << 4 },
    { label: "S", full: "Sat", bit: 1 << 5 },
    { label: "S", full: "Sun", bit: 1 << 6 },
];

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

function formatDisplayTime(min: number): string {
    const h = Math.floor(min / 60);
    const m = min % 60;
    const padM = m.toString().padStart(2, "0");
    const period = h >= 12 ? "PM" : "AM";
    const h12 = h % 12 === 0 ? 12 : h % 12;
    return `${h12}:${padM} ${period}`;
}

function isCurrentlyActive(schedule: ScheduleDto): boolean {
    if (!schedule.enabled) return false;
    const now = new Date();
    // Monday = 0, ..., Sunday = 6
    const day = (now.getDay() + 6) % 7;
    const currentMinute = now.getHours() * 60 + now.getMinutes();

    if (schedule.start_minute <= schedule.end_minute) {
        // Same-day window
        const dayActive = (schedule.weekday_mask & (1 << day)) !== 0;
        return dayActive && currentMinute >= schedule.start_minute && currentMinute < schedule.end_minute;
    } else {
        // Overnight window
        if (currentMinute >= schedule.start_minute) {
            return (schedule.weekday_mask & (1 << day)) !== 0;
        } else if (currentMinute < schedule.end_minute) {
            const prevDay = (day + 6) % 7;
            return (schedule.weekday_mask & (1 << prevDay)) !== 0;
        }
        return false;
    }
}

export function DowntimeSection({ catalog, notify }: DowntimeSectionProps) {
    const { t } = useTranslation();
    const {
        schedules,
        allowlist,
        loading,
        createSchedule,
        updateSchedule,
        toggleSchedule,
        deleteSchedule,
        setAllowlist,
    } = useDowntime(notify);

    const [editingSchedule, setEditingSchedule] = useState<ScheduleDto | null | "new">(null);
    const [allowlistSearch, setAllowlistSearch] = useState("");
    const [selectedAppId, setSelectedAppId] = useState<number | "">("");

    const activeCount = schedules.filter((s) => s.enabled).length;
    const anyActiveNow = schedules.some(isCurrentlyActive);

    const metrics: MetricData[] = [
        {
            label: t("downtime.activeSchedules", "Active Schedules"),
            value: `${activeCount} / ${schedules.length}`,
            icon: <Calendar size={16} />,
        },
        {
            label: t("downtime.status", "Downtime Status"),
            value: anyActiveNow
                ? t("downtime.statusActive", "Active now")
                : t("downtime.statusInactive", "Inactive"),
            icon: <Moon size={16} color={anyActiveNow ? "var(--accent-amber)" : "var(--text-secondary)"} />,
            badge: anyActiveNow ? "Enforcing" : undefined,
        },
        {
            label: t("downtime.alwaysAllowed", "Always Allowed"),
            value: allowlist.length,
            icon: <ShieldCheck size={16} color="var(--accent-emerald)" />,
        },
    ];

    // Filter available apps for allowlist dropdown
    const availableApps = useMemo(() => {
        if (!catalog) return [];
        const allowlistAppIds = new Set(
            allowlist
                .filter((item) => item.subject_type === "app")
                .map((item) => item.subject_id)
        );
        return catalog.apps
            .filter((a) => !allowlistAppIds.has(a.id))
            .filter((a) =>
                a.display_name.toLowerCase().includes(allowlistSearch.toLowerCase())
            )
            .sort((a, b) => a.display_name.localeCompare(b.display_name));
    }, [catalog, allowlist, allowlistSearch]);

    const handleAddAppToAllowlist = async () => {
        if (selectedAppId === "") return;
        try {
            await setAllowlist("app", Number(selectedAppId), true);
            setSelectedAppId("");
            setAllowlistSearch("");
            notify("success", "App added to downtime allowlist.");
        } catch {
            // Handled in hook
        }
    };

    const handleRemoveAllowlist = async (subjectType: string, subjectId: number) => {
        try {
            await setAllowlist(subjectType, subjectId, false);
            notify("success", "App removed from allowlist.");
        } catch {
            // Handled in hook
        }
    };

    return (
        <div className="downtime-section">
            <MetricCards metrics={metrics} />

            {/* Schedules List Card */}
            <div className="card glass-card" style={{ marginTop: "24px", padding: "24px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "20px" }}>
                    <div>
                        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                            <Moon size={18} color="var(--accent-indigo)" />
                            <h3 style={{ margin: 0, fontSize: "16px", fontWeight: 600, color: "var(--text-primary)" }}>
                                {t("downtime.title", "Scheduled Downtime")}
                            </h3>
                        </div>
                        <p style={{ margin: "4px 0 0 0", fontSize: "13px", color: "var(--text-secondary)" }}>
                            {t("downtime.subtitle", "Set recurring bedtime or focus windows during which non-allowlisted apps are locked.")}
                        </p>
                    </div>
                    <button
                        type="button"
                        className="btn btn-primary btn-sm"
                        onClick={() => setEditingSchedule("new")}
                    >
                        <Plus size={14} />
                        {t("downtime.newSchedule", "New Schedule")}
                    </button>
                </div>

                {loading && schedules.length === 0 ? (
                    <div style={{ textAlign: "center", padding: "40px" }}>
                        <LoadingSpinner size="md" />
                    </div>
                ) : schedules.length === 0 ? (
                    <div className="empty-card" style={{ padding: "36px 20px", textAlign: "center", background: "var(--bg-surface)", borderRadius: "var(--radius-md)", border: "1px dashed var(--border-subtle)" }}>
                        <Moon size={32} style={{ opacity: 0.35, marginBottom: "12px", color: "var(--accent-indigo)" }} />
                        <h4 style={{ margin: "0 0 6px 0", color: "var(--text-primary)", fontSize: "15px" }}>
                            {t("downtime.noSchedules", "No downtime schedules configured yet.")}
                        </h4>
                        <p style={{ margin: 0, fontSize: "13px", color: "var(--text-secondary)", maxWidth: "420px" }}>
                            {t("downtime.noSchedulesDesc", "Create a bedtime or focus schedule to automatically lock distracting apps during rest hours.")}
                        </p>
                        <button
                            type="button"
                            className="btn btn-primary btn-sm"
                            style={{ marginTop: "16px" }}
                            onClick={() => setEditingSchedule("new")}
                        >
                            <Plus size={14} />
                            {t("downtime.newSchedule", "Create Bedtime Schedule")}
                        </button>
                    </div>
                ) : (
                    <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
                        {schedules.map((schedule) => {
                            const isOvernight = schedule.start_minute > schedule.end_minute;
                            const activeNow = isCurrentlyActive(schedule);
                            return (
                                <div
                                    key={schedule.id}
                                    className="schedule-row"
                                    style={{
                                        display: "flex",
                                        alignItems: "center",
                                        justifyContent: "space-between",
                                        padding: "16px 20px",
                                        background: activeNow ? "rgba(99, 102, 241, 0.08)" : "var(--bg-surface)",
                                        border: activeNow ? "1px solid var(--accent-indigo)" : "1px solid var(--border-subtle)",
                                        borderRadius: "var(--radius-md)",
                                        transition: "all 0.2s ease",
                                    }}
                                >
                                    <div style={{ display: "flex", alignItems: "center", gap: "16px", flex: 1 }}>
                                        <ToggleSwitch
                                            checked={schedule.enabled}
                                            onChange={(checked) => toggleSchedule(schedule.id, checked)}
                                        />

                                        <div>
                                            <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                                                <span style={{ fontSize: "15px", fontWeight: 600, color: "var(--text-primary)" }}>
                                                    {schedule.name}
                                                </span>
                                                {activeNow && (
                                                    <span className="badge badge--indigo badge-sm">
                                                        Active Now
                                                    </span>
                                                )}
                                                {isOvernight && (
                                                    <span className="badge badge-sm" style={{ background: "var(--bg-surface-raised)", color: "var(--text-secondary)" }}>
                                                        Overnight
                                                    </span>
                                                )}
                                            </div>

                                            <div style={{ display: "flex", alignItems: "center", gap: "12px", marginTop: "6px" }}>
                                                <div style={{ display: "flex", alignItems: "center", gap: "6px", fontFamily: "var(--font-mono)", fontSize: "13px", color: "var(--text-primary)" }}>
                                                    <Clock size={13} color="var(--text-muted)" />
                                                    <span>{formatDisplayTime(schedule.start_minute)}</span>
                                                    <span style={{ color: "var(--text-muted)" }}>–</span>
                                                    <span>{formatDisplayTime(schedule.end_minute)}</span>
                                                </div>

                                                <div style={{ display: "flex", gap: "4px" }}>
                                                    {WEEKDAYS.map((w, idx) => {
                                                        const active = (schedule.weekday_mask & w.bit) !== 0;
                                                        return (
                                                            <span
                                                                key={idx}
                                                                title={w.full}
                                                                style={{
                                                                    width: "20px",
                                                                    height: "20px",
                                                                    display: "inline-flex",
                                                                    alignItems: "center",
                                                                    justifyContent: "center",
                                                                    borderRadius: "4px",
                                                                    fontSize: "10px",
                                                                    fontWeight: 600,
                                                                    fontFamily: "var(--font-mono)",
                                                                    background: active ? "var(--accent-indigo)" : "var(--bg-surface-raised)",
                                                                    color: active ? "#ffffff" : "var(--text-muted)",
                                                                    opacity: active ? 1 : 0.45,
                                                                }}
                                                            >
                                                                {w.label}
                                                            </span>
                                                        );
                                                    })}
                                                </div>
                                            </div>
                                        </div>
                                    </div>

                                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm"
                                            onClick={() => setEditingSchedule(schedule)}
                                            title={t("downtime.editSchedule", "Edit Schedule")}
                                        >
                                            <Edit2 size={14} />
                                        </button>
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm text-danger"
                                            onClick={() => deleteSchedule(schedule.id)}
                                            title={t("downtime.deleteSchedule", "Delete Schedule")}
                                        >
                                            <Trash2 size={14} />
                                        </button>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>

            {/* Always Allowed Apps Card */}
            <div className="card glass-card" style={{ marginTop: "24px", padding: "24px" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "4px" }}>
                    <ShieldCheck size={18} color="var(--accent-emerald)" />
                    <h3 style={{ margin: 0, fontSize: "16px", fontWeight: 600, color: "var(--text-primary)" }}>
                        {t("downtime.allowlistTitle", "Always Allowed Apps")}
                    </h3>
                </div>
                <p style={{ margin: "0 0 16px 0", fontSize: "13px", color: "var(--text-secondary)" }}>
                    {t("downtime.allowlistDesc", "Essential applications chosen here will never be blocked during downtime schedules (e.g. Phone, Calculator, Notes).")}
                </p>

                {/* Add app selector */}
                <div style={{ display: "flex", gap: "10px", marginBottom: "16px", flexWrap: "wrap" }}>
                    <div style={{ flex: 1, minWidth: "220px", position: "relative" }}>
                        <select
                            className="form-select"
                            value={selectedAppId}
                            onChange={(e) => setSelectedAppId(e.target.value ? Number(e.target.value) : "")}
                            style={{ width: "100%" }}
                        >
                            <option value="">{t("downtime.searchAppToAllow", "Choose an app to allow...")}</option>
                            {availableApps.map((app) => (
                                <option key={app.id} value={app.id}>
                                    {app.display_name}
                                </option>
                            ))}
                        </select>
                    </div>

                    <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        disabled={selectedAppId === ""}
                        onClick={handleAddAppToAllowlist}
                    >
                        <Plus size={14} />
                        {t("downtime.addToAllowlist", "Add to Allowlist")}
                    </button>
                </div>

                {/* Current allowlist items */}
                {allowlist.length === 0 ? (
                    <div style={{ padding: "16px", textAlign: "center", color: "var(--text-muted)", fontSize: "13px", fontStyle: "italic", background: "var(--bg-surface)", borderRadius: "var(--radius-sm)" }}>
                        {t("downtime.noAllowlist", "No apps currently allowlisted. All non-exempt apps will be locked during scheduled downtime.")}
                    </div>
                ) : (
                    <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                        {allowlist.map((item) => (
                            <div
                                key={`${item.subject_type}-${item.subject_id}`}
                                style={{
                                    display: "inline-flex",
                                    alignItems: "center",
                                    gap: "8px",
                                    padding: "6px 12px",
                                    background: "var(--bg-surface)",
                                    border: "1px solid var(--border-subtle)",
                                    borderRadius: "var(--radius-sm)",
                                    fontSize: "13px",
                                    color: "var(--text-primary)",
                                }}
                            >
                                <span style={{ width: "6px", height: "6px", borderRadius: "50%", background: "var(--accent-emerald)" }} />
                                <span>{item.name}</span>
                                <button
                                    type="button"
                                    className="btn-ghost"
                                    style={{ padding: "2px", border: "none", cursor: "pointer", display: "inline-flex", color: "var(--text-muted)" }}
                                    onClick={() => handleRemoveAllowlist(item.subject_type, item.subject_id)}
                                    title="Remove from allowlist"
                                >
                                    <X size={12} />
                                </button>
                            </div>
                        ))}
                    </div>
                )}
            </div>

            {/* Schedule Editor Modal */}
            {editingSchedule && (
                <ScheduleEditorModal
                    schedule={editingSchedule === "new" ? null : editingSchedule}
                    onSave={async (name, weekdayMask, startMin, endMin) => {
                        if (editingSchedule === "new") {
                            await createSchedule(name, weekdayMask, startMin, endMin);
                        } else {
                            await updateSchedule(editingSchedule.id, name, weekdayMask, startMin, endMin);
                        }
                        setEditingSchedule(null);
                    }}
                    onClose={() => setEditingSchedule(null)}
                />
            )}
        </div>
    );
}

interface ScheduleEditorModalProps {
    schedule: ScheduleDto | null;
    onSave: (name: string, weekdayMask: number, startMinute: number, endMinute: number) => Promise<void>;
    onClose: () => void;
}

function ScheduleEditorModal({ schedule, onSave, onClose }: ScheduleEditorModalProps) {
    const { t } = useTranslation();
    const [name, setName] = useState(schedule?.name || "Bedtime");
    const [startTime, setStartTime] = useState(minuteToTimeString(schedule?.start_minute ?? 1320)); // 22:00
    const [endTime, setEndTime] = useState(minuteToTimeString(schedule?.end_minute ?? 420)); // 07:00
    const [weekdayMask, setWeekdayMask] = useState(schedule?.weekday_mask ?? 127); // All 7 days
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const startMinute = timeStringToMinute(startTime);
    const endMinute = timeStringToMinute(endTime);
    const isOvernight = startMinute > endMinute;

    const toggleDay = (bit: number) => {
        setWeekdayMask((prev) => prev ^ bit);
    };

    const handleSave = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!name.trim()) {
            setError("Schedule name cannot be empty.");
            return;
        }
        if (weekdayMask === 0) {
            setError("Please select at least one active day.");
            return;
        }
        try {
            setSaving(true);
            setError(null);
            await onSave(name.trim(), weekdayMask, startMinute, endMinute);
        } catch (err: any) {
            setError(err?.message || "Failed to save schedule.");
        } finally {
            setSaving(false);
        }
    };

    return (
        <Dialog label={schedule ? "Edit Schedule" : "New Schedule"} onClose={onClose}>
            <form onSubmit={handleSave} className="dialog-content">
                <div className="dialog-eyebrow">Downtime & Bedtime</div>
                <h3 className="dialog-title">
                    {schedule ? t("downtime.editSchedule", "Edit Schedule") : t("downtime.newSchedule", "New Schedule")}
                </h3>

                {error && (
                    <div className="dialog-error" style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                        <AlertCircle size={14} />
                        <span>{error}</span>
                    </div>
                )}

                {/* Name */}
                <div className="field">
                    <label className="field-label">{t("downtime.scheduleName", "Schedule Name")}</label>
                    <input
                        type="text"
                        className="form-input"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder="e.g. Bedtime, Deep Focus"
                        required
                    />
                </div>

                {/* Time Range */}
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px", marginTop: "14px" }}>
                    <div className="field" style={{ margin: 0 }}>
                        <label className="field-label">{t("downtime.startTime", "Start Time")}</label>
                        <input
                            type="time"
                            className="form-input"
                            value={startTime}
                            onChange={(e) => setStartTime(e.target.value)}
                            required
                        />
                    </div>
                    <div className="field" style={{ margin: 0 }}>
                        <label className="field-label">{t("downtime.endTime", "End Time")}</label>
                        <input
                            type="time"
                            className="form-input"
                            value={endTime}
                            onChange={(e) => setEndTime(e.target.value)}
                            required
                        />
                    </div>
                </div>

                {isOvernight && (
                    <div style={{ marginTop: "10px", fontSize: "12px", color: "var(--accent-indigo)", display: "flex", alignItems: "center", gap: "6px" }}>
                        <Sparkles size={13} />
                        <span>{t("downtime.overnightNotice", "Overnight window: runs from evening through the following morning.")}</span>
                    </div>
                )}

                {/* Weekdays */}
                <div className="field" style={{ marginTop: "18px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                        <label className="field-label">{t("downtime.activeDays", "Active Days")}</label>
                        <div style={{ display: "flex", gap: "6px", fontSize: "11px" }}>
                            <button
                                type="button"
                                className="btn-ghost"
                                style={{ padding: "1px 6px", fontSize: "11px", cursor: "pointer", color: "var(--accent-indigo)" }}
                                onClick={() => setWeekdayMask(127)}
                            >
                                {t("downtime.everyday", "Every day")}
                            </button>
                            <span style={{ color: "var(--text-muted)" }}>•</span>
                            <button
                                type="button"
                                className="btn-ghost"
                                style={{ padding: "1px 6px", fontSize: "11px", cursor: "pointer", color: "var(--accent-indigo)" }}
                                onClick={() => setWeekdayMask(31)}
                            >
                                {t("downtime.weekdays", "Weekdays")}
                            </button>
                            <span style={{ color: "var(--text-muted)" }}>•</span>
                            <button
                                type="button"
                                className="btn-ghost"
                                style={{ padding: "1px 6px", fontSize: "11px", cursor: "pointer", color: "var(--accent-indigo)" }}
                                onClick={() => setWeekdayMask(96)}
                            >
                                {t("downtime.weekends", "Weekends")}
                            </button>
                        </div>
                    </div>

                    <div style={{ display: "flex", gap: "8px", marginTop: "8px" }}>
                        {WEEKDAYS.map((w, idx) => {
                            const isSelected = (weekdayMask & w.bit) !== 0;
                            return (
                                <button
                                    key={idx}
                                    type="button"
                                    onClick={() => toggleDay(w.bit)}
                                    style={{
                                        flex: 1,
                                        height: "36px",
                                        borderRadius: "var(--radius-sm)",
                                        border: isSelected
                                            ? "1px solid var(--accent-indigo)"
                                            : "1px solid var(--border-strong)",
                                        background: isSelected
                                            ? "var(--accent-indigo)"
                                            : "var(--bg-input)",
                                        color: isSelected ? "#ffffff" : "var(--text-secondary)",
                                        fontWeight: 600,
                                        fontSize: "13px",
                                        fontFamily: "var(--font-mono)",
                                        cursor: "pointer",
                                        transition: "all 0.15s ease",
                                    }}
                                >
                                    {w.label}
                                </button>
                            );
                        })}
                    </div>
                </div>

                {/* Actions */}
                <div className="dialog-actions" style={{ marginTop: "24px" }}>
                    <button type="button" className="btn btn-secondary" onClick={onClose} disabled={saving}>
                        Cancel
                    </button>
                    <button type="submit" className="btn btn-primary" disabled={saving}>
                        {saving && <LoadingSpinner size="xs" />}
                        Save Schedule
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
