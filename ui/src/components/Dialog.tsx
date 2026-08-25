import { useEffect, useRef, type ReactNode } from "react";

const FOCUSABLE = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    "[tabindex]:not([tabindex='-1'])",
].join(", ");

const stack: HTMLDivElement[] = [];

interface DialogProps {
    label: string;
    onClose: () => void;
    children: ReactNode;
}

export function Dialog({ label, onClose, children }: DialogProps) {
    const cardRef = useRef<HTMLDivElement | null>(null);
    const onCloseRef = useRef(onClose);
    onCloseRef.current = onClose;

    useEffect(() => {
        const card = cardRef.current;
        if (!card) return;
        const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        stack.push(card);

        const first = card.querySelector<HTMLElement>(FOCUSABLE);
        if (first) first.focus();
        else card.focus();

        const onKeyDown = (event: KeyboardEvent) => {
            if (stack[stack.length - 1] !== card) return;
            if (event.key === "Escape") {
                event.preventDefault();
                onCloseRef.current();
                return;
            }
            if (event.key !== "Tab") return;
            const nodes = Array.from(card.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
                (el) => el.offsetWidth > 0 || el.offsetHeight > 0,
            );
            if (nodes.length === 0) {
                event.preventDefault();
                card.focus();
                return;
            }
            const index = nodes.findIndex((el) => el === document.activeElement);
            event.preventDefault();
            const next = event.shiftKey
                ? index <= 0
                    ? nodes.length - 1
                    : index - 1
                : index === -1 || index === nodes.length - 1
                  ? 0
                  : index + 1;
            nodes[next]?.focus();
        };

        document.addEventListener("keydown", onKeyDown);
        return () => {
            document.removeEventListener("keydown", onKeyDown);
            const at = stack.indexOf(card);
            if (at !== -1) stack.splice(at, 1);
            opener?.focus();
        };
    }, []);

    return (
        <div
            className="backdrop"
            role="presentation"
            onMouseDown={(event) => {
                if (event.target === event.currentTarget) onCloseRef.current();
            }}
        >
            <div ref={cardRef} className="dialog" role="dialog" aria-modal="true" aria-label={label} tabIndex={-1}>
                {children}
            </div>
        </div>
    );
}
