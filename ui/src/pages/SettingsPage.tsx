import { useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Volume2 } from "lucide-react";
import { applyLanguage } from "../i18n";
import { previewAlertSound } from "../api";
import type { useLedgerActions } from "../hooks/useLedgerActions";
import type { ThemePreference } from "../hooks/useTheme";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { StatusDto } from "../types/generated/StatusDto";

type Actions = ReturnType<typeof useLedgerActions>;

/** The help topics under About and help, in reading order. */
const HELP_TOPICS = ["today", "budgets", "pin", "schedules", "websites", "timer"] as const;

interface SettingsPageProps {
    statusInfo: StatusDto | null;
    catalog: CatalogDto | null;
    actions: Actions;
    onSetSetting: (key: string, value: string) => Promise<void>;
    settingPending: (key: string) => boolean;
    onOpenAppDirectory: () => void;
    theme: ThemePreference;
    setTheme: (theme: ThemePreference) => void;
}

const DEFAULT_HOTKEY = "Ctrl+Alt+T";

/** One labelled row: title and hint on the start side, control on the end. */
function Row({ title, hint, children, id }: { title: string; hint?: string; children: ReactNode; id?: string }) {
    return (
        <div className="tt-row">
            <span className="tt-row-text">
                <span className="tt-row-title" id={id}>{title}</span>
                {hint !== undefined && <span className="tt-row-detail">{hint}</span>}
            </span>
            <span className="tt-row-end">{children}</span>
        </div>
    );
}

function Switch({ checked, label, disabled, onChange }: { checked: boolean; label: string; disabled?: boolean; onChange: (next: boolean) => void }) {
    return (
        <input
            type="checkbox"
            role="switch"
            className="toggle-switch"
            checked={checked}
            disabled={disabled}
            aria-label={label}
            onChange={(e) => onChange(e.target.checked)}
        />
    );
}

/** A number saved when the field loses focus, if it changed. */
function NumberField({
    label,
    value,
    min,
    max,
    unit,
    disabled,
    onCommit,
}: {
    label: string;
    value: number | undefined;
    min: number;
    max: number;
    unit: string;
    disabled: boolean;
    onCommit: (value: string) => void;
}) {
    return (
        <span className="tt-value">
            <input
                type="number"
                aria-label={label}
                min={min}
                max={max}
                key={`${label}_${value}`}
                defaultValue={value?.toString() ?? ""}
                disabled={disabled}
                onBlur={(e) => {
                    if (e.target.value !== value?.toString()) onCommit(e.target.value);
                }}
            />
            <span className="tt-unit">{unit}</span>
        </span>
    );
}

function minutesToTime(minutes: number): string {
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

export function SettingsPage({
    statusInfo,
    catalog,
    actions,
    onSetSetting,
    settingPending,
    onOpenAppDirectory,
    theme,
    setTheme,
}: SettingsPageProps) {
    const { t, i18n } = useTranslation();
    const [recordingHotkey, setRecordingHotkey] = useState(false);
    const [playing, setPlaying] = useState(false);
    const [localVolume, setLocalVolume] = useState<number | null>(null);
    const [helpOpen, setHelpOpen] = useState(false);

    const set = (key: string, value: string) => void onSetSetting(key, value);
    const volume = localVolume ?? statusInfo?.alert_volume ?? 80;
    const profile = statusInfo?.profile ?? "self";
    const hotkey = statusInfo?.hud_peek_hotkey || DEFAULT_HOTKEY;

    const recordHotkey = (e: KeyboardEvent) => {
        if (!recordingHotkey) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") {
            setRecordingHotkey(false);
            return;
        }
        if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
        const parts: string[] = [];
        if (e.ctrlKey) parts.push("Ctrl");
        if (e.altKey) parts.push("Alt");
        if (e.shiftKey) parts.push("Shift");
        if (e.metaKey) parts.push("Win");
        const named: Record<string, string> = { " ": "Space", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right" };
        parts.push(named[e.key] ?? e.key.toUpperCase());
        setRecordingHotkey(false);
        set("hud_peek_hotkey", parts.join("+"));
    };

    const commitVolume = (value: string) => set("alert_volume", value);

    const playChime = async () => {
        setPlaying(true);
        try {
            await previewAlertSound(volume);
        } finally {
            setTimeout(() => setPlaying(false), 850);
        }
    };

    return (
        <div className="tt-page">
            <div className="tt-head">
                <div>
                    <h1 className="tt-title">{t("settingsPage.title")}</h1>
                </div>
            </div>

            <div className="tt-settings-grid">
                <section className="tt-card" aria-labelledby="set-protection">
                    <h2 id="set-protection" className="tt-card-title">{t("settingsPage.protection")}</h2>
                    <div>
                        <Row title={t("settingsPage.profile")} hint={t("settingsPage.profileHint")}>
                            <span className="tt-segmented" role="group" aria-label={t("settingsPage.profile")}>
                                <button type="button" aria-pressed={profile === "self"} disabled={settingPending("profile")} onClick={() => set("profile", "self")}>
                                    {t("settingsPage.profileSelf")}
                                </button>
                                <button type="button" aria-pressed={profile === "guardian"} disabled={settingPending("profile")} onClick={() => set("profile", "guardian")}>
                                    {t("settingsPage.profileGuardian")}
                                </button>
                            </span>
                        </Row>
                        <Row title={t("settingsPage.pin")} hint={statusInfo?.pin_configured ? t("settingsPage.pinSet") : t("settingsPage.pinNotSet")}>
                            <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" onClick={actions.openPinSetup}>
                                {statusInfo?.pin_configured ? t("settingsPage.changePin") : t("settingsPage.setPin")}
                            </button>
                        </Row>
                        <Row title={t("settingsPage.strict")} hint={t("settingsPage.strictHint")}>
                            <Switch
                                checked={statusInfo?.strict_mode ?? false}
                                disabled={settingPending("strict_mode")}
                                label={t("settingsPage.strict")}
                                onChange={(v) => set("strict_mode", String(v))}
                            />
                        </Row>
                        <Row title={t("settingsPage.cooldown")} hint={t("settingsPage.cooldownHint")}>
                            <NumberField
                                label={t("settingsPage.cooldown")}
                                value={statusInfo?.limit_cooldown_hours}
                                min={0}
                                max={168}
                                unit={t("settingsPage.hours")}
                                disabled={settingPending("limit_cooldown_hours")}
                                onCommit={(v) => set("limit_cooldown_hours", v)}
                            />
                        </Row>
                    </div>
                </section>

                <section className="tt-card" aria-labelledby="set-timer">
                    <h2 id="set-timer" className="tt-card-title">{t("settingsPage.timer")}</h2>
                    <div>
                        <Row title={t("settingsPage.showTimer")} hint={t("settingsPage.showTimerHint")}>
                            <Switch
                                checked={statusInfo?.show_hud_overlay ?? true}
                                disabled={settingPending("show_hud_overlay")}
                                label={t("settingsPage.showTimer")}
                                onChange={(v) => set("show_hud_overlay", String(v))}
                            />
                        </Row>
                        <Row title={t("settingsPage.fullscreen")} hint={t("settingsPage.fullscreenHint")}>
                            <Switch
                                checked={statusInfo?.show_hud_in_fullscreen ?? false}
                                disabled={settingPending("show_hud_in_fullscreen")}
                                label={t("settingsPage.fullscreen")}
                                onChange={(v) => set("show_hud_in_fullscreen", String(v))}
                            />
                        </Row>
                        <Row title={t("settingsPage.peek")} hint={t("settingsPage.peekHint")}>
                            <button
                                type="button"
                                className={`tt-btn tt-btn--sm ${recordingHotkey ? "tt-btn--primary" : "tt-btn--outline"}`}
                                aria-label={t("settingsPage.peekChange", { shortcut: hotkey })}
                                onClick={() => setRecordingHotkey((r) => !r)}
                                onKeyDown={recordHotkey}
                                onBlur={() => setRecordingHotkey(false)}
                            >
                                {recordingHotkey ? t("settings.pressKeys") : hotkey}
                            </button>
                            {hotkey !== DEFAULT_HOTKEY && (
                                <button type="button" className="tt-btn tt-btn--ghost tt-btn--sm" onClick={() => set("hud_peek_hotkey", DEFAULT_HOTKEY)}>
                                    {t("common.reset")}
                                </button>
                            )}
                        </Row>
                        <Row title={t("settingsPage.chime")} hint={t("settingsPage.chimeHint")}>
                            <input
                                type="range"
                                className="tt-range"
                                min={0}
                                max={100}
                                step={5}
                                value={volume}
                                aria-label={t("settingsPage.chime")}
                                aria-valuetext={`${volume}%`}
                                disabled={settingPending("alert_volume")}
                                onChange={(e) => setLocalVolume(Number.parseInt(e.target.value, 10))}
                                onMouseUp={(e) => commitVolume((e.target as HTMLInputElement).value)}
                                onTouchEnd={(e) => commitVolume((e.target as HTMLInputElement).value)}
                                onKeyUp={(e) => commitVolume((e.target as HTMLInputElement).value)}
                            />
                            <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" disabled={playing} onClick={() => void playChime()}>
                                <Volume2 size={15} aria-hidden="true" />
                                {playing ? t("settingsPage.playing") : t("settingsPage.play")}
                            </button>
                        </Row>
                    </div>
                </section>

                <section className="tt-card" aria-labelledby="set-day">
                    <h2 id="set-day" className="tt-card-title">{t("settingsPage.day")}</h2>
                    <div>
                        <Row title={t("settingsPage.dayStart")} hint={t("settingsPage.dayStartHint")}>
                            <span className="tt-value">
                                <input
                                    type="time"
                                    aria-label={t("settingsPage.dayStart")}
                                    key={`daystart_${statusInfo?.day_start_minutes}`}
                                    defaultValue={minutesToTime(statusInfo?.day_start_minutes ?? 0)}
                                    disabled={settingPending("day_start_minutes")}
                                    style={{ width: 110 }}
                                    onBlur={(e) => {
                                        const [h, m] = e.target.value.split(":").map((x) => Number.parseInt(x, 10));
                                        if (h === undefined || m === undefined || Number.isNaN(h) || Number.isNaN(m)) return;
                                        const minutes = h * 60 + m;
                                        if (minutes !== statusInfo?.day_start_minutes) set("day_start_minutes", String(minutes));
                                    }}
                                />
                            </span>
                        </Row>
                        <Row title={t("settingsPage.idle")} hint={t("settingsPage.idleHint")}>
                            <NumberField
                                label={t("settingsPage.idle")}
                                value={statusInfo?.idle_threshold_secs}
                                min={5}
                                max={3600}
                                unit={t("settingsPage.seconds")}
                                disabled={settingPending("idle_threshold_secs")}
                                onCommit={(v) => set("idle_threshold_secs", v)}
                            />
                        </Row>
                    </div>
                </section>

                <section className="tt-card" aria-labelledby="set-look">
                    <h2 id="set-look" className="tt-card-title">{t("settingsPage.look")}</h2>
                    <div>
                        <Row title={t("settingsPage.language")}>
                            <span className="tt-segmented" role="group" aria-label={t("settingsPage.language")}>
                                <button type="button" aria-pressed={i18n.language?.startsWith("en") ?? false} onClick={() => applyLanguage("en")}>
                                    English
                                </button>
                                <button type="button" lang="ar" aria-pressed={i18n.language?.startsWith("ar") ?? false} onClick={() => applyLanguage("ar")}>
                                    العربية
                                </button>
                            </span>
                        </Row>
                        <Row title={t("settingsPage.theme")}>
                            <span className="tt-segmented" role="group" aria-label={t("settingsPage.theme")}>
                                <button type="button" aria-pressed={theme === "light"} onClick={() => setTheme("light")}>
                                    {t("settingsPage.themeLight")}
                                </button>
                                <button type="button" aria-pressed={theme === "dark"} onClick={() => setTheme("dark")}>
                                    {t("settingsPage.themeDark")}
                                </button>
                                <button type="button" aria-pressed={theme === "system"} onClick={() => setTheme("system")}>
                                    {t("settingsPage.themeSystem")}
                                </button>
                            </span>
                        </Row>
                    </div>
                </section>

                <section className="tt-card" aria-labelledby="set-apps">
                    <h2 id="set-apps" className="tt-card-title">{t("settingsPage.apps")}</h2>
                    <div>
                        <Row
                            title={t("settingsPage.appDirectory")}
                            hint={catalog?.apps.length ? t("categorize.appsDetected", { count: catalog.apps.length }) : t("settingsPage.appDirectoryHint")}
                        >
                            <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" onClick={onOpenAppDirectory}>
                                {t("settingsPage.openApps")}
                            </button>
                        </Row>
                    </div>
                </section>

                <section className="tt-card" aria-labelledby="set-help">
                    <div className="tt-card-head">
                        <h2 id="set-help" className="tt-card-title">{t("settingsPage.aboutHelp")}</h2>
                        <button
                            type="button"
                            className="tt-btn tt-btn--ghost tt-btn--sm"
                            aria-expanded={helpOpen}
                            aria-controls="set-help-body"
                            onClick={() => setHelpOpen((o) => !o)}
                        >
                            {helpOpen ? t("settingsPage.hideHelp") : t("settingsPage.showHelp")}
                            <ChevronDown size={16} aria-hidden="true" style={{ transform: helpOpen ? "rotate(180deg)" : undefined }} />
                        </button>
                    </div>
                    <p className="tt-sub">
                        {statusInfo ? t("settingsPage.version", { version: statusInfo.agent_version }) : t("settingsPage.versionUnknown")}
                    </p>
                    <ul id="set-help-body" className="tt-list" hidden={!helpOpen}>
                        {HELP_TOPICS.map((topic) => (
                            <li className="tt-row" key={topic}>
                                <span className="tt-row-text">
                                    <span className="tt-row-title">{t(`help.${topic}Title`)}</span>
                                    <span className="tt-row-detail">
                                        {t(`help.${topic}`, { hotkey: statusInfo?.hud_peek_hotkey ?? "Ctrl+Alt+T" })}
                                    </span>
                                </span>
                            </li>
                        ))}
                    </ul>
                </section>
            </div>
        </div>
    );
}
