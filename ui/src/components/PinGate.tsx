import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { describeError, recoverPin } from "../api";
import { Dialog } from "./Dialog";

interface PinGateProps {
    label: string;
    error: string | null;
    busy: boolean;
    onSubmit: (pin: string) => void;
    onClose: () => void;
}

export function PinGate({ label, error, busy, onSubmit, onClose }: PinGateProps) {
    const { t } = useTranslation();
    const [mode, setMode] = useState<"pin" | "forgot">("pin");
    const [pin, setPin] = useState("");
    const [code, setCode] = useState("");
    const [newPin, setNewPin] = useState("");
    const [confirmPin, setConfirmPin] = useState("");
    const [localError, setLocalError] = useState<string | null>(null);
    const [recovering, setRecovering] = useState(false);

    const submit = (event: FormEvent) => {
        event.preventDefault();
        const value = pin.trim();
        if (value.length === 0 || busy) return;
        setPin("");
        onSubmit(value);
    };

    const submitRecovery = async (event: FormEvent) => {
        event.preventDefault();
        if (recovering) return;
        if (code.trim().length === 0 || newPin.trim().length === 0) {
            setLocalError(t("pinGate.errFill"));
            return;
        }
        if (newPin !== confirmPin) {
            setLocalError(t("pinGate.errDiff"));
            return;
        }
        setLocalError(null);
        setRecovering(true);
        try {
            await recoverPin(code.trim(), newPin.trim());
            // Vault rotated: back to sign-in with the PIN they now know.
            setMode("pin");
            setCode("");
            setNewPin("");
            setConfirmPin("");
            setPin("");
            setLocalError(null);
        } catch (e: unknown) {
            setLocalError(describeError(e));
        } finally {
            setRecovering(false);
        }
    };

    const guardedClose = () => {
        if (!busy && !recovering) onClose();
    };

    return (
        <Dialog label={t("pinGate.title")} onClose={guardedClose}>
            {mode === "pin" ? (
                <form onSubmit={submit}>
                    <p className="dialog-eyebrow">{t("pinGate.title")}</p>
                    <h2 className="dialog-title">{label}</h2>
                    <input
                        className="pin-input"
                        type="password"
                        inputMode="numeric"
                        autoComplete="off"
                        placeholder="····"
                        value={pin}
                        disabled={busy}
                        aria-label="PIN"
                        onChange={(event) => setPin(event.target.value)}
                    />
                    {error !== null && <p className="dialog-error">{error}</p>}
                    {localError !== null && mode === "pin" && (
                        <p className="dialog-error">{localError}</p>
                    )}
                    <div className="dialog-actions">
                        <button
                            type="submit"
                            className="btn btn--primary"
                            disabled={busy || pin.trim().length === 0}
                        >
                            {busy ? t("pinGate.checking") : t("pinGate.confirm")}
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
                    <button
                        type="button"
                        className="linklike"
                        onClick={() => {
                            setLocalError(null);
                            setMode("forgot");
                        }}
                        disabled={busy || recovering}
                    >
                        {t("pinGate.forgot")}
                    </button>
                </form>
            ) : (
                <form onSubmit={(e) => void submitRecovery(e)}>
                    <p className="dialog-eyebrow">{t("pinGate.recovery")}</p>
                    <h2 className="dialog-title">{t("pinGate.forgotTitle")}</h2>
                    <p className="panel-note">
                        {t("pinGate.forgotDesc")}
                    </p>
                    <label className="field">
                        <span className="field-label">{t("pinGate.recoveryCode")}</span>
                        <input
                            className="recovery-input"
                            type="text"
                            autoComplete="off"
                            spellCheck={false}
                            placeholder="XXXX-XXXX-XXXX-XXXX"
                            value={code}
                            disabled={recovering}
                            aria-label="Recovery code"
                            onChange={(event) => {
                                setCode(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    <label className="field">
                        <span className="field-label">{t("pinGate.newPin")}</span>
                        <input
                            type="password"
                            inputMode="numeric"
                            autoComplete="new-password"
                            value={newPin}
                            disabled={recovering}
                            aria-label="New PIN"
                            onChange={(event) => {
                                setNewPin(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    <label className="field">
                        <span className="field-label">{t("pinGate.repeatPin")}</span>
                        <input
                            type="password"
                            inputMode="numeric"
                            autoComplete="new-password"
                            value={confirmPin}
                            disabled={recovering}
                            aria-label="Repeat new PIN"
                            onChange={(event) => {
                                setConfirmPin(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    {(localError !== null || error !== null) && (
                        <p className="dialog-error">{localError ?? error}</p>
                    )}
                    <div className="dialog-actions">
                        <button
                            type="submit"
                            className="btn btn--primary"
                            disabled={recovering}
                        >
                            {recovering ? t("pinGate.replacing") : t("pinGate.replacePin")}
                        </button>
                        <button
                            type="button"
                            className="btn btn--secondary"
                            onClick={() => {
                                setLocalError(null);
                                setMode("pin");
                            }}
                            disabled={recovering}
                        >
                            {t("pinGate.back")}
                        </button>
                    </div>
                </form>
            )}
        </Dialog>
    );
}
