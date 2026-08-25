import type { ReactNode } from "react";

interface SectionProps {
    label: string;
    children: ReactNode;
}

export function Section({ label, children }: SectionProps) {
    return (
        <section className="section">
            <header className="section-head">
                <h2 className="eyebrow">{label}</h2>
            </header>
            {children}
        </section>
    );
}
