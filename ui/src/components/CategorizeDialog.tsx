import { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { colorForCategory } from "../categoryColors";
import { Dialog } from "./Dialog";
import { ChevronDownIcon } from "./icons/Icons";

interface CategorizeDialogProps {
    appName: string;
    currentPrimaryId: number | null;
    currentTagIds: number[];
    catalog: CatalogDto | null;
    busy: boolean;
    onClose: () => void;
    onCategorize: (primaryId: number, tagIds: number[]) => void;
    onAutoDetect: () => void;
}

interface CategoryOption {
    value: number | null;
    label: string;
    color?: string;
}

function CategorySelect({
    options,
    value,
    disabled,
    onChange,
}: {
    options: CategoryOption[];
    value: number | null;
    disabled: boolean;
    onChange: (val: number | null) => void;
}) {
    const [open, setOpen] = useState(false);
    const containerRef = useRef<HTMLDivElement>(null);
    const selected = options.find((o) => o.value === value);

    useEffect(() => {
        if (!open) return;
        const handler = (e: MouseEvent) => {
            if (!containerRef.current?.contains(e.target as Node)) {
                setOpen(false);
            }
        };
        document.addEventListener("mousedown", handler);
        return () => document.removeEventListener("mousedown", handler);
    }, [open]);

    return (
        <div ref={containerRef} className="app-picker" style={{ position: "relative" }}>
            <button
                type="button"
                className="app-picker__trigger"
                disabled={disabled}
                onClick={() => setOpen((prev) => !prev)}
                aria-haspopup="listbox"
                aria-expanded={open}
            >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    {selected?.color && (
                        <span style={{ width: 8, height: 8, borderRadius: "50%", backgroundColor: selected.color, flexShrink: 0 }} />
                    )}
                    <span>{selected ? selected.label : "Auto-detect"}</span>
                </div>
                <ChevronDownIcon size={14} />
            </button>

            {open && (
                <div
                    className="app-picker__dropdown"
                    style={{
                        position: "absolute",
                        top: "calc(100% + 4px)",
                        left: 0,
                        right: 0,
                        zIndex: 100,
                        background: "var(--bg-card-elevated)",
                        border: "1px solid var(--border-subtle)",
                        borderRadius: "10px",
                        boxShadow: "0 12px 32px var(--shadow-modal, rgba(0, 0, 0, 0.4))",
                        maxHeight: "220px",
                        overflowY: "auto",
                        padding: "6px",
                    }}
                >
                    <ul role="listbox" style={{ listStyle: "none", margin: 0, padding: 0 }}>
                        {options.map((opt) => {
                            const isSelected = opt.value === value;
                            return (
                                <li
                                    key={opt.value ?? "auto"}
                                    role="option"
                                    aria-selected={isSelected}
                                    onClick={() => {
                                        onChange(opt.value);
                                        setOpen(false);
                                    }}
                                    style={{
                                        padding: "8px 12px",
                                        borderRadius: "6px",
                                        cursor: "pointer",
                                        fontSize: "13px",
                                        color: isSelected ? "var(--color-accent)" : "var(--text-primary)",
                                        background: isSelected ? "var(--bg-surface-hover)" : "transparent",
                                        display: "flex",
                                        alignItems: "center",
                                        justifyContent: "space-between",
                                        fontWeight: isSelected ? 600 : 400,
                                    }}
                                    onMouseEnter={(e) => {
                                        if (!isSelected) (e.currentTarget as HTMLElement).style.background = "var(--bg-surface-hover)";
                                    }}
                                    onMouseLeave={(e) => {
                                        if (!isSelected) (e.currentTarget as HTMLElement).style.background = "transparent";
                                    }}
                                >
                                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                        {opt.color && (
                                            <span style={{ width: 8, height: 8, borderRadius: "50%", backgroundColor: opt.color, flexShrink: 0 }} />
                                        )}
                                        <span>{opt.label}</span>
                                    </div>
                                    {isSelected && <span style={{ fontSize: "11px", color: "var(--color-accent)" }}>✓</span>}
                                </li>
                            );
                        })}
                    </ul>
                </div>
            )}
        </div>
    );
}

export function CategorizeDialog({
    appName,
    currentPrimaryId,
    currentTagIds,
    catalog,
    busy,
    onClose,
    onCategorize,
    onAutoDetect,
}: CategorizeDialogProps) {
    const { t } = useTranslation();
    const [selectedPrimary, setSelectedPrimary] = useState<number | null>(currentPrimaryId);
    const [selectedTags, setSelectedTags] = useState<number[]>(currentTagIds ?? []);

    const limitableCategories = (catalog?.categories ?? []).filter((c) => c.kind === "limitable");

    const categoryOptions: CategoryOption[] = [
        { value: null, label: t("categorize.autoDetect") },
        ...limitableCategories.map((c) => ({ 
            value: c.id, 
            label: c.name, 
            color: colorForCategory(c.name, c.color) 
        })),
    ];

    const toggleTag = (tagId: number) => {
        setSelectedTags((prev) =>
            prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId],
        );
    };

    const handleSave = () => {
        if (selectedPrimary !== null) {
            onCategorize(selectedPrimary, selectedTags);
        } else {
            onAutoDetect();
        }
    };

    return (
        <Dialog label={t("categorize.title")} onClose={onClose}>
            <p className="dialog-eyebrow">{t("categorize.eyebrow")}</p>
            <h2 className="dialog-title">{appName}</h2>
            <p className="panel-note">
                {t("categorize.desc")}
            </p>

            <div className="field">
                <span className="field-label">{t("categorize.primary")}</span>
                <CategorySelect
                    options={categoryOptions}
                    value={selectedPrimary}
                    disabled={busy}
                    onChange={(val) => setSelectedPrimary(val)}
                />
            </div>

            {limitableCategories.length > 0 && selectedPrimary !== null && catalog?.categories.find(c => c.id === selectedPrimary)?.slug !== "uncategorized" && (
                <div className="field">
                    <span className="field-label">{t("categorize.tags")}</span>
                    <div className="tag-list">
                        {limitableCategories.map((category) => {
                            const color = colorForCategory(category.name, category.color);
                            return (
                                <label key={category.id} className="tag-item">
                                    <input
                                        type="checkbox"
                                        checked={selectedTags.includes(category.id)}
                                        disabled={busy || category.id === selectedPrimary}
                                        onChange={() => toggleTag(category.id)}
                                    />
                                    <span style={{ width: 8, height: 8, borderRadius: "50%", backgroundColor: color, flexShrink: 0 }} />
                                    <span>{category.name}</span>
                                </label>
                            );
                        })}
                    </div>
                </div>
            )}

            <div className="dialog-actions">
                <button
                    type="button"
                    className="btn btn--secondary"
                    onClick={onAutoDetect}
                    disabled={busy}
                >
                    {t("categorize.autoDetect")}
                </button>
                <button
                    type="button"
                    className="btn btn--primary"
                    onClick={handleSave}
                    disabled={busy}
                >
                    {busy ? t("categorize.saving") : t("common.save")}
                </button>
            </div>
        </Dialog>
    );
}
