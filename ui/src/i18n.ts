import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en/translation.json";
import ar from "./locales/ar/translation.json";

// Try to read saved language from localStorage (set by settings).
// Falls back to English on any error.
const savedLang = (() => {
    try {
        return localStorage.getItem("screentime_language") || "en";
    } catch {
        return "en";
    }
})();

i18n
    .use(initReactI18next)
    .init({
        resources: {
            en: { translation: en },
            ar: { translation: ar },
        },
        lng: savedLang,
        fallbackLng: "en",
        interpolation: {
            escapeValue: false, // React already escapes
        },
    });

export default i18n;

/**
 * Persist the language choice to localStorage and update the document direction.
 */
export function applyLanguage(lang: string) {
    i18n.changeLanguage(lang);
    localStorage.setItem("screentime_language", lang);
    document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";
    document.documentElement.lang = lang;
}

/**
 * Apply document direction on initial load.
 */
export function initDirection() {
    const lang = i18n.language || "en";
    document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";
    document.documentElement.lang = lang;
}
