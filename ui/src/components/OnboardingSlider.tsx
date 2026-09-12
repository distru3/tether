import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { applyLanguage } from "../i18n";
import { Hand, Globe, BarChart3, Clock, Target, ShieldCheck } from "lucide-react";
import { getStatus, setSetting } from "../api";
import { ToggleSwitch } from "./ToggleSwitch";
import { LoadingSpinner } from "./LoadingSpinner";
import "./OnboardingSlider.css";

interface OnboardingSliderProps {
    onComplete: () => void;
}

export function OnboardingSlider({ onComplete }: OnboardingSliderProps) {
    const { t, i18n } = useTranslation();
    const [currentSlide, setCurrentSlide] = useState(0);
    const [familyDns, setFamilyDns] = useState(false);
    const [dnsLoading, setDnsLoading] = useState(false);

    useEffect(() => {
        getStatus()
            .then((s) => setFamilyDns(s.family_dns_enabled))
            .catch(() => {});
    }, []);

    async function handleToggleFamilyDns(enabled: boolean) {
        setDnsLoading(true);
        try {
            await setSetting("family_dns", enabled ? "true" : "false");
            setFamilyDns(enabled);
        } catch {
            // fail-safe ignore
        } finally {
            setDnsLoading(false);
        }
    }

    const slides = [
        {
            icon: <Hand size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.welcome"),
            description: t("onboarding.welcomeDesc"),
            extra: null,
        },
        {
            icon: <Globe size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.chooseLanguage"),
            description: t("onboarding.chooseLanguageDesc"),
            extra: (
                <div className="onboarding-lang-picker">
                    <button
                        type="button"
                        className={`lang-option ${i18n.language === "en" ? "lang-option--active" : ""}`}
                        onClick={() => applyLanguage("en")}
                    >
                        English
                    </button>
                    <button
                        type="button"
                        className={`lang-option ${i18n.language === "ar" ? "lang-option--active" : ""}`}
                        onClick={() => applyLanguage("ar")}
                    >
                        العربية
                    </button>
                </div>
            ),
        },
        {
            icon: <BarChart3 size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.trackTime"),
            description: t("onboarding.trackTimeDesc"),
            extra: null,
        },
        {
            icon: <Clock size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.setLimits"),
            description: t("onboarding.setLimitsDesc"),
            extra: null,
        },
        {
            icon: <ShieldCheck size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.familyDnsTitle"),
            description: t("onboarding.familyDnsDesc"),
            extra: (
                <div className="onboarding-dns-card">
                    <div className="onboarding-dns-info">
                        <span className="onboarding-dns-label">{t("onboarding.familyDnsEnable")}</span>
                        <span className="onboarding-dns-hint">{t("onboarding.familyDnsEnabledDesc")}</span>
                        <div className="onboarding-dns-status">
                            <span className={`status-dot ${familyDns ? "status-dot-active" : "status-dot-inactive"}`} />
                            <span className="status-label">{familyDns ? t("settings.familyDnsActive") : t("settings.familyDnsInactive")}</span>
                        </div>
                    </div>
                    <div className="onboarding-dns-switch-wrap">
                        {dnsLoading ? (
                            <LoadingSpinner size="sm" />
                        ) : (
                            <ToggleSwitch
                                checked={familyDns}
                                onChange={handleToggleFamilyDns}
                            />
                        )}
                    </div>
                </div>
            ),
        },
        {
            icon: <Target size={64} className="onboarding-lucide-icon" />,
            title: t("onboarding.stayFocused"),
            description: t("onboarding.stayFocusedDesc"),
            extra: null,
        },
    ];

    const isLast = currentSlide === slides.length - 1;

    function handleNext() {
        if (isLast) {
            onComplete();
        } else {
            setCurrentSlide((prev) => prev + 1);
        }
    }

    function handleSkip() {
        onComplete();
    }

    const slide = slides[currentSlide]!;

    return (
        <div className="onboarding-fullscreen">
            <div className="onboarding-container">
                <div className="onboarding-slide" key={currentSlide}>
                    <div className="onboarding-icon-wrapper">{slide.icon}</div>
                    <h1 className="onboarding-title">{slide.title}</h1>
                    <p className="onboarding-desc">{slide.description}</p>
                    {slide.extra}
                </div>

                <div className="onboarding-bottom-bar">
                    <div className="onboarding-dots">
                        {slides.map((_, i) => (
                            <span
                                key={i}
                                className={`onboarding-dot ${i === currentSlide ? "onboarding-dot--active" : ""}`}
                            />
                        ))}
                    </div>

                    <div className="onboarding-actions">
                        <button type="button" className="btn btn-ghost" onClick={handleSkip}>
                            {t("onboarding.skip")}
                        </button>
                        <button type="button" className="btn btn-primary" onClick={handleNext}>
                            {isLast ? t("onboarding.getStarted") : t("onboarding.next")}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
