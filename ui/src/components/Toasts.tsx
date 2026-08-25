import type { ToastItem } from "../hooks/useToasts";

interface ToastsProps {
    toasts: ToastItem[];
    dismiss: (id: number) => void;
}

export function Toasts({ toasts, dismiss }: ToastsProps) {
    return (
        <div className="toasts" role="region" aria-live="polite" aria-label="Notices">
            {toasts.map((toast) => (
                <button
                    key={toast.id}
                    type="button"
                    className={`toast toast--${toast.kind}`}
                    title="Dismiss"
                    onClick={() => dismiss(toast.id)}
                >
                    {toast.message}
                </button>
            ))}
        </div>
    );
}
