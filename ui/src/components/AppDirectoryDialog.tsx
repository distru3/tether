import { useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AppWindow, Search, X, Tag } from "lucide-react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { colorForCategory } from "../categoryColors";
import { Dialog } from "./Dialog";

interface AppDirectoryDialogProps {
    catalog: CatalogDto | null;
    busy: boolean;
    onClose: () => void;
    onCategorize: (appId: number, appName: string, primaryId: number | null, tagIds: number[]) => void;
}

type FilterMode = "all" | "uncategorized" | "categorized";

export function AppDirectoryDialog({
    catalog,
    busy,
    onClose,
    onCategorize,
}: AppDirectoryDialogProps) {
    const { t } = useTranslation();
    const [search, setSearch] = useState("");
    const [filter, setFilter] = useState<FilterMode>("all");

    const categoriesMap = useMemo(() => {
        const map = new Map<number, { name: string; color?: string; slug: string }>();
        if (catalog) {
            for (const cat of catalog.categories) {
                map.set(cat.id, { name: cat.name, color: cat.color, slug: cat.slug });
            }
        }
        return map;
    }, [catalog]);

    const appsList = useMemo(() => {
        if (!catalog) return [];
        return [...catalog.apps].sort((a, b) => a.display_name.localeCompare(b.display_name));
    }, [catalog]);

    const uncategorizedCount = useMemo(() => {
        return appsList.filter((app) => {
            const cat = categoriesMap.get(app.primary_category);
            return !cat || cat.slug === "uncategorized" || cat.name.toLowerCase() === "uncategorized";
        }).length;
    }, [appsList, categoriesMap]);

    const categorizedCount = appsList.length - uncategorizedCount;

    const filteredApps = useMemo(() => {
        const query = search.trim().toLowerCase();
        return appsList.filter((app) => {
            const cat = categoriesMap.get(app.primary_category);
            const isUncat = !cat || cat.slug === "uncategorized" || cat.name.toLowerCase() === "uncategorized";

            if (filter === "uncategorized" && !isUncat) return false;
            if (filter === "categorized" && isUncat) return false;

            if (query) {
                const matchesName = app.display_name.toLowerCase().includes(query) || app.key.toLowerCase().includes(query);
                const matchesCat = cat?.name.toLowerCase().includes(query);
                return matchesName || matchesCat;
            }

            return true;
        });
    }, [appsList, categoriesMap, search, filter]);

    return (
        <Dialog label={t("categorize.appDirectory", "Application Directory")} onClose={onClose}>
            <div className="app-directory-dialog">
                <p className="dialog-eyebrow">{t("categorize.eyebrow", "App Classification")}</p>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                    <h2 className="dialog-title">{t("categorize.appDirectory", "Application Directory")}</h2>
                    <span style={{ fontSize: "12px", color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
                        {t("categorize.appsDetected", { count: appsList.length })}
                    </span>
                </div>
                <p className="panel-note" style={{ marginBottom: 16 }}>
                    {t("categorize.desc", "Choose the primary category for this app. You can also add tag categories for overlapping budgets.")}
                </p>

                {/* Search & filter toolbar */}
                <div className="app-directory-toolbar">
                    <div className="app-directory-search-wrapper">
                        <Search size={14} className="app-directory-search-icon" />
                        <input
                            type="text"
                            className="app-directory-search-input"
                            placeholder={t("categorize.searchApps", "Search applications...")}
                            value={search}
                            onChange={(e) => setSearch(e.target.value)}
                            autoFocus
                        />
                        {search && (
                            <button
                                type="button"
                                className="app-directory-search-clear"
                                onClick={() => setSearch("")}
                                title="Clear search"
                            >
                                <X size={13} />
                            </button>
                        )}
                    </div>

                    <div className="app-directory-filter-tabs">
                        <button
                            type="button"
                            className={`app-directory-tab ${filter === "all" ? "app-directory-tab--active" : ""}`}
                            onClick={() => setFilter("all")}
                        >
                            {t("categorize.filterAll", "All")} ({appsList.length})
                        </button>
                        <button
                            type="button"
                            className={`app-directory-tab ${filter === "categorized" ? "app-directory-tab--active" : ""}`}
                            onClick={() => setFilter("categorized")}
                        >
                            {t("categorize.filterCategorized", "Categorized")} ({categorizedCount})
                        </button>
                        <button
                            type="button"
                            className={`app-directory-tab ${filter === "uncategorized" ? "app-directory-tab--active" : ""}`}
                            onClick={() => setFilter("uncategorized")}
                        >
                            {t("categorize.filterUncategorized", "Uncategorized")} ({uncategorizedCount})
                        </button>
                    </div>
                </div>

                {/* Applications list */}
                <div className="app-directory-list">
                    {filteredApps.length === 0 ? (
                        <div className="app-directory-empty">
                            <Tag size={24} style={{ opacity: 0.4, marginBottom: 8 }} />
                            <p>{t("categorize.noAppsFound", "No applications match your search.")}</p>
                        </div>
                    ) : (
                        filteredApps.map((app) => {
                            const cat = categoriesMap.get(app.primary_category);
                            const catName = cat?.name ?? "Uncategorized";
                            const catColor = colorForCategory(catName, cat?.color);

                            return (
                                <div key={app.id} className="app-directory-item">
                                    <div className="app-directory-item-main">
                                        <div
                                            className="app-directory-icon-box"
                                            style={{
                                                backgroundColor: `${catColor}24`,
                                                color: catColor,
                                                borderColor: `${catColor}44`,
                                            }}
                                        >
                                            <AppWindow size={16} />
                                        </div>
                                        <div className="app-directory-item-info">
                                            <div className="app-directory-name-row">
                                                <span className="app-directory-name">{app.display_name}</span>
                                                {app.user_classified && (
                                                    <span className="app-directory-custom-badge" title="Manually customized by user">
                                                        {t("categorize.customized", "Custom")}
                                                    </span>
                                                )}
                                            </div>
                                            <span className="app-directory-key">{app.key}</span>
                                        </div>
                                    </div>

                                    <div className="app-directory-item-actions">
                                        <button
                                            type="button"
                                            className="target-badge target-badge--category-tag target-badge--clickable"
                                            style={{
                                                backgroundColor: `${catColor}22`,
                                                color: catColor,
                                                borderColor: `${catColor}44`,
                                                cursor: "pointer",
                                            }}
                                            onClick={() => onCategorize(app.id, app.display_name, app.primary_category, app.tags)}
                                            title={t("categorize.changeCategory", "Click to change category")}
                                            disabled={busy}
                                        >
                                            {catName}
                                        </button>
                                        <button
                                            type="button"
                                            className="btn btn-ghost btn-sm app-directory-categorize-btn"
                                            onClick={() => onCategorize(app.id, app.display_name, app.primary_category, app.tags)}
                                            disabled={busy}
                                        >
                                            {t("ledger.tag", "Tag")}
                                        </button>
                                    </div>
                                </div>
                            );
                        })
                    )}
                </div>

                <div className="dialog-actions" style={{ marginTop: 16 }}>
                    <button type="button" className="btn btn--secondary" onClick={onClose}>
                        {t("common.close", "Close")}
                    </button>
                </div>
            </div>
        </Dialog>
    );
}
