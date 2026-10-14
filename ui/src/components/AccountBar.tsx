import { useTranslation } from "react-i18next";
import { UserRound } from "lucide-react";

import type { AccountDto } from "../types/generated/AccountDto";

interface AccountBarProps {
    accounts: AccountDto[];
    /** This window's own account, from Status. */
    own: AccountDto | null;
    /** The account on screen; null is `own`. */
    viewing: number | null;
    onView: (accountId: number | null) => void;
}

/**
 * Whose screen time Today and Activity show, on a PC with more than one
 * Windows account (docs/adr/0005-per-account-usage.md). Also says so when
 * limits don't apply to the account on screen.
 */
export function AccountBar({ accounts, own, viewing, onView }: AccountBarProps) {
    const { t } = useTranslation();
    const shown = viewing === null ? own : (accounts.find((a) => a.id === viewing) ?? null);
    const several = accounts.length > 1;
    const unwatched = shown !== null && !shown.supervised;
    if (!several && !unwatched) return null;

    const other = viewing !== null && viewing !== own?.id;
    const label = (a: AccountDto) => (a.id === own?.id ? t("accounts.you", { name: a.name }) : a.name);

    return (
        <div className={`tt-account-bar${other ? " tt-account-bar--other" : ""}`} role="region" aria-label={t("accounts.region")}>
            <span className="tt-account-avatar" aria-hidden="true">
                {shown ? shown.name.trim().charAt(0).toUpperCase() : <UserRound size={14} />}
            </span>
            <span className="tt-account-text">
                {other && shown ? t("accounts.viewingOther", { name: shown.name }) : t("accounts.showing")}
            </span>
            {several && (
                <select
                    className="tt-select tt-account-select"
                    aria-label={t("accounts.pick")}
                    value={shown?.id ?? ""}
                    onChange={(e) => onView(Number(e.target.value))}
                >
                    {accounts.map((a) => (
                        <option key={a.id} value={a.id}>
                            {label(a)}
                        </option>
                    ))}
                </select>
            )}
            {unwatched && <span className="tt-account-note">{t("accounts.unwatched")}</span>}
            {other && (
                <button type="button" className="tt-btn tt-btn--ghost tt-btn--sm" onClick={() => onView(null)}>
                    {t("accounts.backToMine")}
                </button>
            )}
        </div>
    );
}
