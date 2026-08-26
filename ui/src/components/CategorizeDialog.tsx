import { useState } from "react";

import type { CatalogDto } from "../types/generated/CatalogDto";
import { Dialog } from "./Dialog";

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
    const [selectedPrimary, setSelectedPrimary] = useState<number | null>(currentPrimaryId);
    const [selectedTags, setSelectedTags] = useState<number[]>(currentTagIds ?? []);

    const limitableCategories = (catalog?.categories ?? []).filter((c) => c.kind === "limitable");

    const toggleTag = (tagId: number) => {
        setSelectedTags((prev) =>
            prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId],
        );
    };

    const handleSave = () => {
        if (selectedPrimary !== null) {
            onCategorize(selectedPrimary, selectedTags);
        }
    };

    return (
        <Dialog label="Categorize app" onClose={onClose}>
            <p className="dialog-eyebrow">App classification</p>
            <h2 className="dialog-title">{appName}</h2>
            <p className="panel-note">
                Choose the primary category for this app. You can also add tag categories for
                overlapping budgets (e.g., TikTok as both Short-Form Video and Social Media).
            </p>

            <label className="field">
                <span className="field-label">Primary category</span>
                <select
                    value={selectedPrimary ?? ""}
                    disabled={busy}
                    onChange={(event) => {
                        const value = event.target.value;
                        setSelectedPrimary(value === "" ? null : parseInt(value, 10));
                    }}
                >
                    <option value="">Auto-detect</option>
                    {limitableCategories.map((category) => (
                        <option key={category.id} value={category.id}>
                            {category.name}
                        </option>
                    ))}
                </select>
            </label>

            {limitableCategories.length > 0 && (
                <div className="field">
                    <span className="field-label">Tag categories (optional)</span>
                    <div className="tag-list">
                        {limitableCategories.map((category) => (
                            <label key={category.id} className="tag-item">
                                <input
                                    type="checkbox"
                                    checked={selectedTags.includes(category.id)}
                                    disabled={busy || category.id === selectedPrimary}
                                    onChange={() => toggleTag(category.id)}
                                />
                                <span>{category.name}</span>
                            </label>
                        ))}
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
                    Auto-detect
                </button>
                <button
                    type="button"
                    className="btn btn--primary"
                    onClick={handleSave}
                    disabled={busy || selectedPrimary === null}
                >
                    {busy ? "Saving…" : "Save"}
                </button>
            </div>
        </Dialog>
    );
}
