import { useCallback, useEffect, useRef, useState } from "react";

export type ToastKind = "info" | "success" | "error";

export interface ToastItem {
    id: number;
    kind: ToastKind;
    message: string;
}

const TOAST_MS = 5000;

export function useToasts() {
    const [toasts, setToasts] = useState<ToastItem[]>([]);
    const timers = useRef(new Map<number, number>());
    const seq = useRef(0);

    const dismiss = useCallback((id: number) => {
        const timer = timers.current.get(id);
        if (timer !== undefined) {
            window.clearTimeout(timer);
            timers.current.delete(id);
        }
        setToasts((current) => current.filter((toast) => toast.id !== id));
    }, []);

    const push = useCallback((kind: ToastKind, message: string) => {
        seq.current += 1;
        const id = seq.current;
        setToasts((current) => [...current, { id, kind, message }]);
        const timer = window.setTimeout(() => {
            timers.current.delete(id);
            setToasts((current) => current.filter((toast) => toast.id !== id));
        }, TOAST_MS);
        timers.current.set(id, timer);
    }, []);

    useEffect(
        () => () => {
            for (const timer of timers.current.values()) window.clearTimeout(timer);
            timers.current.clear();
        },
        [],
    );

    return { toasts, push, dismiss };
}
