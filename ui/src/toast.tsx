import { useCallback, useState } from "react";
import type { Toast, ToastKind } from "./types";

const TOAST_MS = 4500;
let toastSeq = 0;

interface ToastHost {
    toasts: Toast[];
    push: (kind: ToastKind, message: string) => void;
}

/** A self-contained toast stack: render `<ToastStack />` once, use `push()`. */
export function useToasts(): ToastHost {
    const [toasts, setToasts] = useState<Toast[]>([]);

    const push = useCallback((kind: ToastKind, message: string) => {
        const id = ++toastSeq;
        setToasts((t) => [...t, { id, kind, message }]);
        window.setTimeout(() => {
            setToasts((t) => t.filter((x) => x.id !== id));
        }, TOAST_MS);
    }, []);

    return { toasts, push };
}

const ICONS: Record<ToastKind, string> = {
    success: "✓",
    error: "✕",
    info: "i",
};

export function ToastStack({ toasts }: { toasts: Toast[] }) {
    return (
        <div className="toasts" aria-live="polite">
            {toasts.map((t) => (
                <div key={t.id} className={`toast toast-${t.kind}`} role="status">
                    <span className="toast-icon">{ICONS[t.kind]}</span>
                    <span className="toast-msg">{t.message}</span>
                </div>
            ))}
        </div>
    );
}