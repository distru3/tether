import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Moon, User, Users } from "lucide-react";
import { applyLanguage } from "../../i18n";
import * as api from "../../api";
import { formatDuration } from "../../format";
import { clockLabel } from "../../limitText";
import type { BudgetHue } from "../../budgetHue";
import type { LimitTargetDto } from "../../types/generated/LimitTargetDto";

type Profile = "self" | "guardian";

interface SetupFlowProps {
    onComplete: () => void;
}

/** A starting point offered on step 2 (there is no usage history yet). */
interface Template {
    id: string;
    hue: BudgetHue | "bedtime";
    /** Category slug, "total", or "bedtime" for the schedule. */
    target: string;
    defaultMinutes: number;
    /** Monday-first; null = same as default. */
    weekdays: (number | null)[];
    on: boolean;
}

/** Monday-first overrides: the default on school days, `weekend` on Sat/Sun. */
const WEEKENDS_AT = (weekend: number) => [null, null, null, null, null, weekend, weekend];

const TEMPLATES: Template[] = [
    { id: "games", hue: "games", target: "games", defaultMinutes: 60, weekdays: WEEKENDS_AT(120), on: true },
    { id: "social", hue: "social", target: "social-media", defaultMinutes: 30, weekdays: [null, null, null, null, null, null, null], on: true },
    { id: "video", hue: "video", target: "video-streaming", defaultMinutes: 90, weekdays: [null, null, null, null, null, null, null], on: true },
    { id: "bedtime", hue: "bedtime", target: "bedtime", defaultMinutes: 0, weekdays: [], on: true },
    { id: "total", hue: "total", target: "total", defaultMinutes: 240, weekdays: [null, null, null, null, null, null, null], on: false },
];

/** Bedtime: 10 pm to 7 am, starting Sunday to Thursday nights (Monday-first bits). */
const BEDTIME = { start: 22 * 60, end: 7 * 60, mask: 0b1001111 };

const STEP_KEYS = ["who", "budgets", "pin", "websites"] as const;

export function SetupFlow({ onComplete }: SetupFlowProps) {
    const { t, i18n } = useTranslation();
    const [step, setStep] = useState(0);
    const [profile, setProfile] = useState<Profile | null>(null);
    const [templates, setTemplates] = useState<Template[]>(TEMPLATES);
    const [busy, setBusy] = useState(false);
    const [notice, setNotice] = useState<string | null>(null);
    const [pinConfigured, setPinConfigured] = useState(false);
    const [familyDns, setFamilyDns] = useState(false);

    useEffect(() => {
        api.getStatus()
            .then((s) => {
                setPinConfigured(s.pin_configured);
                setFamilyDns(s.family_dns_enabled);
                if (s.profile === "guardian") setProfile("guardian");
            })
            .catch(() => {});
    }, []);

    const next = () => {
        setNotice(null);
        setStep((s) => s + 1);
    };

    const saveProfile = async () => {
        if (profile === null) return;
        setBusy(true);
        try {
            await api.setSetting("profile", profile);
        } catch {
            // A PIN from an earlier install refuses this; the choice can be
            // made later in Settings with the PIN.
        } finally {
            setBusy(false);
        }
        next();
    };

    const createBudgets = async () => {
        setBusy(true);
        let failed = 0;
        try {
            const catalog = await api.getCatalog();
            for (const tpl of templates.filter((x) => x.on)) {
                try {
                    if (tpl.target === "bedtime") {
                        await api.createSchedule(t("setup.bedtimeName"), BEDTIME.mask, BEDTIME.start, BEDTIME.end);
                        continue;
                    }
                    let target: LimitTargetDto;
                    if (tpl.target === "total") {
                        target = { kind: "total" };
                    } else {
                        const id = catalog.categories.find((c) => c.slug === tpl.target)?.id;
                        if (id === undefined) {
                            failed += 1;
                            continue;
                        }
                        target = { kind: "category", id };
                    }
                    await api.setLimit(target, tpl.defaultMinutes, tpl.weekdays, true, "");
                } catch {
                    failed += 1;
                }
            }
        } catch {
            failed = templates.filter((x) => x.on).length;
        } finally {
            setBusy(false);
        }
        if (failed > 0) {
            setNotice(t("setup.someFailed", { count: failed }));
            return;
        }
        next();
    };

    return (
        <div className="tt-setup">
            <header className="tt-setup-top">
                <div className="tt-brand">
                    <span className="brand-mark" aria-hidden="true">T</span>
                    <span className="logo-title">Tether</span>
                </div>
                <span className="tt-segmented" role="group" aria-label={t("settingsPage.language")}>
                    <button type="button" aria-pressed={i18n.language?.startsWith("en") ?? false} onClick={() => applyLanguage("en")}>
                        English
                    </button>
                    <button type="button" lang="ar" aria-pressed={i18n.language?.startsWith("ar") ?? false} onClick={() => applyLanguage("ar")}>
                        العربية
                    </button>
                </span>
            </header>

            <main className="tt-setup-main">
                <ol className="tt-steps" aria-label={t("setup.steps")}>
                    {STEP_KEYS.map((key, i) => (
                        <li key={key} aria-current={i === step ? "step" : undefined} className={i < step ? "tt-step--done" : undefined}>
                            <span aria-hidden="true">{i < step ? "✓" : i + 1}</span>
                            {t(`setup.step.${key}`)}
                        </li>
                    ))}
                </ol>

                {step === 0 && (
                    <Step
                        title={t("setup.whoTitle")}
                        body={t("setup.whoBody")}
                        actions={
                            <>
                                <span />
                                <button type="button" className="tt-btn tt-btn--primary" disabled={profile === null || busy} onClick={() => void saveProfile()}>
                                    {t("setup.next")}
                                </button>
                            </>
                        }
                    >
                        <div className="tt-choices" role="radiogroup" aria-label={t("setup.whoTitle")}>
                            <ProfileChoice
                                checked={profile === "self"}
                                onSelect={() => setProfile("self")}
                                icon={<User size={24} />}
                                title={t("setup.whoSelf")}
                                body={t("setup.whoSelfBody")}
                            />
                            <ProfileChoice
                                checked={profile === "guardian"}
                                onSelect={() => setProfile("guardian")}
                                icon={<Users size={24} />}
                                title={t("setup.whoGuardian")}
                                body={t("setup.whoGuardianBody")}
                            />
                        </div>
                    </Step>
                )}

                {step === 1 && (
                    <Step
                        title={t("setup.budgetsTitle")}
                        body={t("setup.budgetsBody")}
                        notice={notice}
                        actions={
                            <>
                                <button type="button" className="tt-btn tt-btn--ghost" onClick={() => setStep(0)}>
                                    {t("setup.back")}
                                </button>
                                <span style={{ display: "flex", gap: 8 }}>
                                    {notice !== null && (
                                        <button type="button" className="tt-btn tt-btn--outline" onClick={next}>
                                            {t("setup.continueAnyway")}
                                        </button>
                                    )}
                                    <button type="button" className="tt-btn tt-btn--primary" disabled={busy} onClick={() => void createBudgets()}>
                                        {t("setup.next")}
                                    </button>
                                </span>
                            </>
                        }
                    >
                        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                            {templates.map((tpl) => (
                                <label key={tpl.id} className={`tt-template tt-hue-${tpl.hue === "bedtime" ? "other" : tpl.hue}`}>
                                    <input
                                        type="checkbox"
                                        checked={tpl.on}
                                        onChange={(e) =>
                                            setTemplates((list) => list.map((x) => (x.id === tpl.id ? { ...x, on: e.target.checked } : x)))
                                        }
                                    />
                                    <span className="tt-swatch" aria-hidden="true">
                                        {tpl.hue === "bedtime" ? <Moon size={18} /> : <span />}
                                    </span>
                                    <span className="tt-row-text">
                                        <span className="tt-row-title">{t(`setup.tpl.${tpl.id}`)}</span>
                                        <span className="tt-row-detail">{t(`setup.tpl.${tpl.id}Hint`)}</span>
                                    </span>
                                    <span className="tt-row-title">{templateAmount(tpl, t)}</span>
                                </label>
                            ))}
                        </div>
                    </Step>
                )}

                {step === 2 && (
                    <PinStep
                        profile={profile ?? "self"}
                        alreadySet={pinConfigured}
                        onBack={() => setStep(1)}
                        onDone={() => {
                            setPinConfigured(true);
                            next();
                        }}
                        onSkip={next}
                    />
                )}

                {step === 3 && (
                    <WebsitesStep
                        familyDns={familyDns}
                        onFamilyDns={setFamilyDns}
                        onBack={() => setStep(2)}
                        onFinish={onComplete}
                    />
                )}
            </main>
        </div>
    );
}

function templateAmount(tpl: Template, t: (key: string, opts?: Record<string, unknown>) => string): string {
    // Same clock format as the rest of the app ("10 PM – 7 AM").
    if (tpl.target === "bedtime") return `${clockLabel(BEDTIME.start)} – ${clockLabel(BEDTIME.end)}`;
    const weekend = tpl.weekdays[5];
    const base = formatDuration(tpl.defaultMinutes * 60);
    if (weekend !== null && weekend !== undefined) {
        return t("setup.schoolAndWeekend", { weekday: base, weekend: formatDuration(weekend * 60) });
    }
    return t("setup.perDay", { amount: base });
}

function Step({
    title,
    body,
    notice,
    actions,
    children,
}: {
    title: string;
    body: string;
    notice?: string | null;
    actions: ReactNode;
    children: ReactNode;
}) {
    return (
        <>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <h1 className="tt-setup-title">{title}</h1>
                <p className="tt-sub">{body}</p>
            </div>
            {children}
            {notice !== null && notice !== undefined && (
                <p className="tt-error" role="alert">
                    {notice}
                </p>
            )}
            <div className="tt-setup-actions">{actions}</div>
        </>
    );
}

function ProfileChoice({
    checked,
    onSelect,
    icon,
    title,
    body,
}: {
    checked: boolean;
    onSelect: () => void;
    icon: ReactNode;
    title: string;
    body: string;
}) {
    return (
        <button type="button" role="radio" aria-checked={checked} className="tt-choice" onClick={onSelect}>
            <span className="tt-choice-icon" aria-hidden="true">{icon}</span>
            <span className="tt-choice-title">{title}</span>
            <span className="tt-choice-body">{body}</span>
        </button>
    );
}

function PinStep({
    profile,
    alreadySet,
    onBack,
    onDone,
    onSkip,
}: {
    profile: Profile;
    alreadySet: boolean;
    onBack: () => void;
    onDone: () => void;
    onSkip: () => void;
}) {
    const { t } = useTranslation();
    const [pin, setPin] = useState("");
    const [confirm, setConfirm] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [code, setCode] = useState<string | null>(null);

    const submit = async (e: FormEvent) => {
        e.preventDefault();
        if (!/^\d{4,}$/.test(pin)) {
            setError(t("setup.pinTooShort"));
            return;
        }
        if (pin !== confirm) {
            setError(t("setup.pinMismatch"));
            return;
        }
        setBusy(true);
        setError(null);
        try {
            const reply = await api.setPin(pin, null);
            setCode(reply.recovery_code);
        } catch (err) {
            setError(api.describeError(err));
        } finally {
            setBusy(false);
        }
    };

    if (code !== null) {
        return (
            <Step
                title={t("setup.codeTitle")}
                body={t("setup.codeBody")}
                actions={
                    <>
                        <span />
                        <button type="button" className="tt-btn tt-btn--primary" onClick={onDone}>
                            {t("setup.codeSaved")}
                        </button>
                    </>
                }
            >
                <p className="tt-code" aria-label={t("setup.codeLabel")}>{code}</p>
            </Step>
        );
    }

    if (alreadySet) {
        return (
            <Step
                title={t("setup.pinTitle")}
                body={t("setup.pinAlreadySet")}
                actions={
                    <>
                        <button type="button" className="tt-btn tt-btn--ghost" onClick={onBack}>{t("setup.back")}</button>
                        <button type="button" className="tt-btn tt-btn--primary" onClick={onSkip}>{t("setup.next")}</button>
                    </>
                }
            >
                <span />
            </Step>
        );
    }

    const required = profile === "guardian";
    return (
        <form onSubmit={(e) => void submit(e)} style={{ display: "contents" }}>
            <Step
                title={t("setup.pinTitle")}
                body={required ? t("setup.pinBodyGuardian") : t("setup.pinBodySelf")}
                notice={error}
                actions={
                    <>
                        <button type="button" className="tt-btn tt-btn--ghost" onClick={onBack}>{t("setup.back")}</button>
                        <span style={{ display: "flex", gap: 8 }}>
                            {!required && (
                                <button type="button" className="tt-btn tt-btn--outline" onClick={onSkip}>{t("setup.skipPin")}</button>
                            )}
                            <button type="submit" className="tt-btn tt-btn--primary" disabled={busy}>{t("setup.savePin")}</button>
                        </span>
                    </>
                }
            >
                <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: 14 }}>
                    <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                        <span className="tt-row-title">{t("setup.pinNew")}</span>
                        <input className="tt-input" type="password" inputMode="numeric" autoComplete="new-password" value={pin} onChange={(e) => setPin(e.target.value)} />
                    </label>
                    <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                        <span className="tt-row-title">{t("setup.pinConfirm")}</span>
                        <input className="tt-input" type="password" inputMode="numeric" autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
                    </label>
                </div>
            </Step>
        </form>
    );
}

function WebsitesStep({
    familyDns,
    onFamilyDns,
    onBack,
    onFinish,
}: {
    familyDns: boolean;
    onFamilyDns: (on: boolean) => void;
    onBack: () => void;
    onFinish: () => void;
}) {
    const { t } = useTranslation();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const toggle = async (on: boolean) => {
        setBusy(true);
        setError(null);
        try {
            await api.setSetting("family_dns", String(on));
            onFamilyDns(on);
        } catch (err) {
            setError(api.describeError(err));
        } finally {
            setBusy(false);
        }
    };

    return (
        <Step
            title={t("setup.webTitle")}
            body={t("setup.webBody")}
            notice={error}
            actions={
                <>
                    <button type="button" className="tt-btn tt-btn--ghost" onClick={onBack}>{t("setup.back")}</button>
                    <button type="button" className="tt-btn tt-btn--primary" onClick={onFinish}>{t("setup.finish")}</button>
                </>
            }
        >
            <div className="tt-card">
                <div className="tt-row" style={{ borderTop: 0 }}>
                    <span className="tt-row-text">
                        <span className="tt-row-title">{t("limitsPage.familyDns")}</span>
                        <span className="tt-row-detail">{t("setup.familyDnsHint")}</span>
                    </span>
                    <input
                        type="checkbox"
                        role="switch"
                        className="toggle-switch"
                        checked={familyDns}
                        disabled={busy}
                        aria-label={t("limitsPage.familyDns")}
                        onChange={(e) => void toggle(e.target.checked)}
                    />
                </div>
                <p className="tt-row-detail">{t("setup.webLater")}</p>
            </div>
        </Step>
    );
}
