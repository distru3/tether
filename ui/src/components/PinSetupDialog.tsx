import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { describeError, removePin, type PinVaultReply } from "../api";
import { Dialog } from "./Dialog";
import { LoadingSpinner } from "./LoadingSpinner";

interface PinSetupDialogProps {
    pinConfigured: boolean;
    busy: boolean;
    onClose: () => void;
    onSubmit: (newPin: string, currentPin: string) => Promise<PinVaultReply>;
}

export function PinSetupDialog({ pinConfigured, busy, onClose, onSubmit }: PinSetupDialogProps) {
    const { t } = useTranslation();
    const [currentPin, setCurrentPin] = useState("");
    const [newPin, setNewPin] = useState("");
    const [confirmPin, setConfirmPin] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [issuedCode, setIssuedCode] = useState<string | null>(null);
    const [removing, setRemoving] = useState(false);
    const [removeCredential, setRemoveCredential] = useState("");

    const submit = (event: FormEvent) => {
        event.preventDefault();
        if (busy) return;
        if (newPin.trim().length === 0) {
            setError(t("pinSetup.errEnterNew"));
            return;
        }
        if (newPin !== confirmPin) {
            setError(t("pinSetup.errDiff"));
            return;
        }
        setError(null);
        onSubmit(newPin.trim(), currentPin.trim())
            .then((reply) => setIssuedCode(reply.recovery_code))
            .catch((e: unknown) => setError(describeError(e)));
    };

    const submitRemove = async () => {
        if (removing) return;
        setRemoving(true);
        try {
            await removePin(removeCredential.trim());
            onClose();
        } catch (e: unknown) {
            setError(describeError(e));
        } finally {
            setRemoving(false);
        }
    };

    const guardedClose = () => {
        if (!busy && !removing) onClose();
    };

    if (issuedCode !== null) {
        return (
            <Dialog label={t("pinSetup.recoveryCodeTitle")} onClose={guardedClose}>
                <p className="dialog-eyebrow">{t("pinSetup.houseKey")}</p>
                <h2 className="dialog-title">{t("pinSetup.writeThisDown")}</h2>
                <p className="panel-note" dangerouslySetInnerHTML={{ __html: t("pinSetup.recoveryDesc") }} />
                <div className="recovery-code" role="textbox" aria-label={t("pinSetup.recoveryCodeTitle")} tabIndex={0}>
                    {issuedCode}
                </div>
                <div className="dialog-actions">
                    <button
                        type="button"
                        className="btn btn--primary"
                        onClick={onClose}
                        disabled={removing}
                    >
                        {t("pinSetup.writtenDown")}
                    </button>
                </div>
            </Dialog>
        );
    }

    return (
        <Dialog label={pinConfigured ? t("pinSetup.changePin") : t("pinSetup.setPin")} onClose={guardedClose}>
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">{t("pinSetup.houseKey")}</p>
                <h2 className="dialog-title">{pinConfigured ? t("pinSetup.changePin") : t("pinSetup.setPin")}</h2>
                <p className="panel-note">
                    {t("pinSetup.pinDesc")}
                </p>
                {pinConfigured && (
                    <label className="field">
                        <span className="field-label">{t("pinSetup.currentPin")}</span>
                        <input
                            type="password"
                            autoComplete="off"
                            value={currentPin}
                            disabled={busy}
                            aria-label={t("pinSetup.currentPin")}
                            onChange={(event) => setCurrentPin(event.target.value)}
                        />
                    </label>
                )}
                <label className="field">
                    <span className="field-label">{t("pinSetup.newPin")}</span>
                    <input
                        type="password"
                        inputMode="numeric"
                        autoComplete="new-password"
                        value={newPin}
                        disabled={busy}
                        aria-label={t("pinSetup.newPin")}
                        onChange={(event) => setNewPin(event.target.value)}
                    />
                </label>
                <label className="field">
                    <span className="field-label">{t("pinSetup.repeatPin")}</span>
                    <input
                        type="password"
                        inputMode="numeric"
                        autoComplete="new-password"
                        value={confirmPin}
                        disabled={busy}
                        aria-label={t("pinSetup.repeatPin")}
                        onChange={(event) => setConfirmPin(event.target.value)}
                    />
                </label>
                {error !== null && <p className="dialog-error">{error}</p>}
                <div className="dialog-actions">
                    <button type="submit" className="btn btn--primary" disabled={busy}>
                        {busy && <LoadingSpinner size="xs" />}
                        {busy ? t("pinSetup.setting") : t("pinSetup.savePin")}
                    </button>
                    <button
                        type="button"
                        className="btn btn--secondary"
                        onClick={guardedClose}
                        disabled={busy}
                    >
                        {t("common.cancel")}
                    </button>
                </div>
            </form>
            {pinConfigured && (
                <div className="remove-vault">
                    <p className="dialog-eyebrow">{t("pinSetup.standDown")}</p>
                    <div className="remove-vault-row">
                        <input
                            type="password"
                            autoComplete="off"
                            placeholder={t("pinSetup.pinOrRecovery")}
                            value={removeCredential}
                            disabled={removing || busy}
                            aria-label={t("pinSetup.pinOrRecovery")}
                            onChange={(event) => {
                                setRemoveCredential(event.target.value);
                                setError(null);
                            }}
                        />
                        <button
                            type="button"
                            className="linklike linklike--danger"
                            disabled={removing || busy || removeCredential.trim().length === 0}
                            onClick={() => void submitRemove()}
                        >
                            {removing && <LoadingSpinner size="xs" />}
                            {removing ? t("pinSetup.removing") : t("pinSetup.removePin")}
                        </button>
                    </div>
                </div>
            )}
        </Dialog>
    );
}
