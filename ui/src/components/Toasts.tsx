import React from "react";
import type { ToastItem } from "../hooks/useToasts";
import { CheckIcon, CloseIcon, WarningIcon, ShieldIcon } from "./icons/Icons";

interface ToastsProps {
    toasts: ToastItem[];
    dismiss: (id: number) => void;
}

export function Toasts({ toasts, dismiss }: ToastsProps) {
    if (toasts.length === 0) return null;

    return (
        <div className="toasts" role="region" aria-live="polite" aria-label="Notices">
            {toasts.map((toast) => {
                const isError = toast.kind === "error";
                const isSuccess = toast.kind === "success";

                return (
                    <div
                        key={toast.id}
                        className={`toast toast--${toast.kind}`}
                        role="alert"
                    >
                        <div className="toast-icon">
                            {isSuccess && <CheckIcon size={16} color="var(--accent-emerald)" />}
                            {isError && <WarningIcon size={16} color="var(--accent-rose)" />}
                            {!isSuccess && !isError && <ShieldIcon size={16} color="var(--accent-indigo)" />}
                        </div>
                        <span className="toast-message">{toast.message}</span>
                        <button
                            type="button"
                            className="toast-dismiss-btn"
                            title="Dismiss notification"
                            onClick={() => dismiss(toast.id)}
                        >
                            <CloseIcon size={14} />
                        </button>
                    </div>
                );
            })}
        </div>
    );
}

