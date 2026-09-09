import { useTranslation } from "react-i18next";

/**
 * Collapsible "How to Use" tutorial, embedded in the Settings tab.
 */
export function TutorialContent() {
    const { t } = useTranslation();

    const steps = [
        { title: t("tutorial.step1_title"), body: t("tutorial.step1") },
        { title: t("tutorial.step2_title"), body: t("tutorial.step2") },
        { title: t("tutorial.step3_title"), body: t("tutorial.step3") },
        { title: t("tutorial.step4_title"), body: t("tutorial.step4") },
        { title: t("tutorial.step5_title"), body: t("tutorial.step5") },
    ];

    return (
        <div className="tutorial-content">
            <p className="form-hint" style={{ marginBottom: 12 }}>
                {t("tutorial.subtitle")}
            </p>
            <ol className="tutorial-steps">
                {steps.map((step, i) => (
                    <li key={i} className="tutorial-step">
                        <strong>{step.title}</strong>
                        <p>{step.body}</p>
                    </li>
                ))}
            </ol>
        </div>
    );
}
