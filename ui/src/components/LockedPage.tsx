import { useTranslation } from "react-i18next";
import { LockKeyhole } from "lucide-react";

/**
 * Limits or Settings in the child view (docs/adr/0006): what the page is
 * for, and an Unlock button for the grown-up who has the PIN.
 */
export function LockedPage({ page, onUnlock }: { page: "limits" | "settings"; onUnlock: () => void }) {
    const { t } = useTranslation();
    return (
        <div className="tt-page">
            <section className="tt-card tt-locked" aria-labelledby="locked-title">
                <span className="tt-locked-icon" aria-hidden="true">
                    <LockKeyhole size={26} />
                </span>
                <h1 id="locked-title" className="tt-title tt-locked-title">
                    {t(`locked.${page}Title`)}
                </h1>
                <p className="tt-sub">{t(`locked.${page}Body`)}</p>
                <button type="button" className="tt-btn tt-btn--primary" onClick={onUnlock}>
                    {t("locked.unlock")}
                </button>
            </section>
        </div>
    );
}
