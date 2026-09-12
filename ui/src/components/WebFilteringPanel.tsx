import { useEffect, useState, useRef, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
    ShieldAlert,
    EyeOff,
    Globe,
    Sparkles,
    Upload,
    Plus,
    Trash2,
    Check,
    AlertCircle,
    Search,
    X,
    ChevronLeft,
    ChevronRight,
} from "lucide-react";
import { addManualBlock, listManualBlocks, removeManualBlock } from "../api";
import { MetricCards } from "./MetricCards";
import { LoadingSpinner } from "./LoadingSpinner";
import "./WebFilteringPanel.css";

const PAGE_SIZE = 10;

export function WebFilteringPanel({ onAttempt }: { onAttempt: (label: string, run: (pin: string) => Promise<void>) => void }) {
    const { t } = useTranslation();
    const [domains, setDomains] = useState<string[]>([]);
    const [newDomain, setNewDomain] = useState("");
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [aiCategory, setAiCategory] = useState("");
    const [promptCopied, setPromptCopied] = useState(false);
    const [isUploading, setIsUploading] = useState(false);
    const [showAdult, setShowAdult] = useState(false);
    const [searchQuery, setSearchQuery] = useState("");
    const [currentPage, setCurrentPage] = useState(1);
    const fileInputRef = useRef<HTMLInputElement>(null);

    const refresh = () => {
        setLoading(true);
        listManualBlocks()
            .then(res => setDomains(res.domains))
            .catch(e => setError(e.toString()))
            .finally(() => setLoading(false));
    };

    useEffect(() => {
        refresh();
    }, []);

    const handleAdd = async (e: React.FormEvent) => {
        e.preventDefault();
        const trimmed = newDomain.trim().toLowerCase();
        if (!trimmed) return;
        
        if (domains.map(d => d.toLowerCase()).includes(trimmed)) {
            setError(t("webFilter.alreadyBlocked"));
            return;
        }

        setError(null);
        try {
            await addManualBlock(trimmed);
            setNewDomain("");
            refresh();
        } catch (e: any) {
            setError(e.toString());
        }
    };

    const handleShowAdult = () => {
        onAttempt(t("pinGate.viewHidden"), async (_pin: string) => {
            setShowAdult(true);
        });
    };

    const isNsfw = (domain: string) => {
        return /(porn|xvideo|xnxx|xhamster|chaturbate|stripchat|onlyfans|redtube|tubegalore|eporner|spankbang|rule34|xxx)/i.test(domain);
    };

    const handleRemove = (domain: string) => {
        setError(null);
        onAttempt(t("pinGate.removeBlock", { domain }), async (pin: string) => {
            await removeManualBlock(domain, pin);
            refresh();
        });
    };

    const handleCopyPrompt = () => {
        if (!aiCategory.trim()) return;
        const prompt = `Please generate a plain text file containing a list of domains related to ${aiCategory.trim()}. Provide ONLY the raw domain names, one per line, with no extra text, numbering, or formatting (e.g. 'example.com'). Do not include subdomains like 'www.' unless necessary. CRITICAL: The list MUST NOT exceed 100 domains to avoid overloading the local DNS resolver.`;
        navigator.clipboard.writeText(prompt).then(() => {
            setPromptCopied(true);
            setTimeout(() => setPromptCopied(false), 2000);
        });
    };

    const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;

        setError(null);
        setIsUploading(true);

        try {
            const text = await file.text();
            const lines = text.split(/[\r\n]+/).map(line => line.trim().toLowerCase()).filter(line => line.length > 0 && !line.startsWith("#"));
            
            const existingLowers = domains.map(d => d.toLowerCase());
            const uniqueNew = Array.from(new Set(lines)).filter(d => !existingLowers.includes(d));

            if (uniqueNew.length === 0) {
                throw new Error(t("webFilter.allBlocked"));
            }

            if (uniqueNew.length > 100) {
                throw new Error(t("webFilter.tooMany", { count: uniqueNew.length }));
            }

            // Simple batch processing
            for (const domain of uniqueNew) {
                if (domain.includes(".") && !domain.includes(" ")) {
                    await addManualBlock(domain);
                }
            }
            refresh();
        } catch (err: any) {
            setError(err.toString());
        } finally {
            setIsUploading(false);
            if (fileInputRef.current) {
                fileInputRef.current.value = "";
            }
        }
    };

    const nsfwCount = domains.filter(isNsfw).length;
    const visibleDomains = useMemo(() => {
        return domains.filter(d => showAdult || !isNsfw(d));
    }, [domains, showAdult]);

    const filteredDomains = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();
        if (!q) return visibleDomains;
        return visibleDomains.filter(d => d.toLowerCase().includes(q));
    }, [visibleDomains, searchQuery]);

    const totalPages = Math.ceil(filteredDomains.length / PAGE_SIZE) || 1;
    const safePage = Math.min(currentPage, totalPages);
    const startIndex = (safePage - 1) * PAGE_SIZE;
    const pageDomains = useMemo(() => {
        return filteredDomains.slice(startIndex, startIndex + PAGE_SIZE);
    }, [filteredDomains, startIndex]);

    const metrics = [
        { label: t("webFilter.totalBlocked", "Total Blocked"), value: domains.length, icon: <ShieldAlert size={16} /> },
        { label: t("webFilter.hiddenDomains", "Hidden Domains"), value: nsfwCount, icon: <EyeOff size={16} /> },
    ];

    return (
        <div className="view-container web-filter-page-shell">
            <div className="view-header page-intro">
                <div>
                    <h2 className="view-title">{t("webFilter.customBlockedDomains", "Web Shield")}</h2>
                    <p className="view-subtitle">{t("webFilter.desc", "Manage blocked domains and bulk upload custom lists.")}</p>
                </div>
            </div>

            <MetricCards metrics={metrics} />

            <section className="card limits-panel web-filter-command-panel">
                <div className="card-body">
                    {error && (
                        <div className="error-text" style={{ marginBottom: "16px", color: "var(--color-danger)", fontSize: "13px", display: "flex", alignItems: "center", gap: "6px" }}>
                            <AlertCircle size={15} />
                            {error}
                        </div>
                    )}

                    <div className="web-filter-controls">
                        <form onSubmit={handleAdd} className="add-domain-form-wrapper">
                            <input
                                type="text"
                                value={newDomain}
                                onChange={e => setNewDomain(e.target.value)}
                                placeholder={t("webFilter.placeholder", "example.com")}
                                className="form-input"
                            />
                            <button type="submit" className="btn btn-primary" disabled={!newDomain.trim()}>
                                <Plus size={15} />
                                {t("webFilter.blockDomain", "Block Domain")}
                            </button>
                        </form>

                        <div className="bulk-upload-section">
                            <input 
                                type="file" 
                                accept=".txt,.csv" 
                                ref={fileInputRef} 
                                style={{ display: 'none' }}
                                onChange={handleFileUpload}
                            />
                            <button 
                                type="button" 
                                className="btn btn-secondary" 
                                onClick={() => fileInputRef.current?.click()}
                                disabled={isUploading}
                            >
                                {isUploading ? <LoadingSpinner size="xs" /> : <Upload size={15} />}
                                {isUploading ? t("webFilter.importing", "Importing...") : t("webFilter.bulkUpload", "Bulk Upload")}
                            </button>
                        </div>
                    </div>

                    <div className="ai-prompt-card">
                        <div className="ai-prompt-header">
                            <Sparkles size={16} color="var(--color-accent)" />
                            <h3 className="ai-prompt-title">{t("webFilter.generateListsWithAi", "Generate Blocklists with AI")}</h3>
                        </div>
                        <p className="ai-prompt-desc">
                            {t("webFilter.aiDesc", "Enter a topic to copy an AI prompt that will generate a formatted domain list for bulk upload.")}
                        </p>
                        <div className="ai-prompt-row">
                            <input
                                type="text"
                                value={aiCategory}
                                onChange={e => setAiCategory(e.target.value)}
                                placeholder={t("webFilter.aiPlaceholder", "e.g. news sites, video streaming...")}
                                className="form-input form-input-sm"
                            />
                            <button 
                                type="button" 
                                className="btn btn-secondary btn-sm" 
                                onClick={handleCopyPrompt}
                                disabled={!aiCategory.trim()}
                            >
                                {promptCopied ? <><Check size={14} /> {t("webFilter.copied", "Copied!")}</> : t("webFilter.copyPrompt", "Copy Prompt")}
                            </button>
                        </div>
                    </div>
                </div>
            </section>

            <section className="card limits-panel web-filter-domain-panel">
                <div className="card-body">
                    <div className="web-filter-table-header">
                        <div className="web-filter-table-title-group">
                            <h3 className="web-filter-table-title">
                                {t("webFilter.activeRules", "Active Domain Rules")}
                            </h3>
                            <span className="web-filter-count-badge">
                                {filteredDomains.length} {filteredDomains.length === 1 ? "rule" : "rules"}
                            </span>
                        </div>
                        <div className="web-filter-search-wrapper">
                            <Search size={14} className="web-filter-search-icon" />
                            <input
                                type="text"
                                value={searchQuery}
                                onChange={e => { setSearchQuery(e.target.value); setCurrentPage(1); }}
                                placeholder={t("webFilter.searchPlaceholder", "Search blocked domains...")}
                                className="web-filter-search-input"
                            />
                            {searchQuery && (
                                <button
                                    type="button"
                                    className="web-filter-search-clear"
                                    onClick={() => { setSearchQuery(""); setCurrentPage(1); }}
                                    title={t("webFilter.clearSearch", "Clear search")}
                                >
                                    <X size={12} />
                                </button>
                            )}
                        </div>
                    </div>

                    <div className="web-filter-table-wrapper">
                        <table className="web-filter-table">
                            <colgroup>
                                <col style={{ width: "45%" }} />
                                <col style={{ width: "23%" }} />
                                <col style={{ width: "16%" }} />
                                <col style={{ width: "16%" }} />
                            </colgroup>
                            <thead>
                                <tr>
                                    <th className="th-domain">{t("webFilter.domain", "Domain")}</th>
                                    <th className="th-category">{t("webFilter.category", "Category")}</th>
                                    <th className="th-status">{t("webFilter.status", "Status")}</th>
                                    <th className="th-action">{t("webFilter.action", "Action")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {loading && !isUploading ? (
                                    <>
                                        {[1, 2, 3, 4, 5].map((i) => (
                                            <tr key={`skeleton-${i}`} className="domain-row domain-row--skeleton">
                                                <td className="td-domain">
                                                    <div className="domain-name-cell">
                                                        <div className="domain-icon-box skeleton-shimmer" style={{ width: 24, height: 24, borderRadius: 6 }} />
                                                        <div className="skeleton-shimmer" style={{ width: `${110 + (i % 3) * 45}px`, height: 16, borderRadius: 4 }} />
                                                    </div>
                                                </td>
                                                <td className="td-category">
                                                    <div className="skeleton-shimmer" style={{ width: 80, height: 22, borderRadius: 12 }} />
                                                </td>
                                                <td className="td-status">
                                                    <div className="skeleton-shimmer" style={{ width: 64, height: 18, borderRadius: 4 }} />
                                                </td>
                                                <td className="td-action">
                                                    <div className="skeleton-shimmer" style={{ width: 56, height: 24, borderRadius: 4, marginLeft: 'auto' }} />
                                                </td>
                                            </tr>
                                        ))}
                                    </>
                                ) : filteredDomains.length === 0 ? (
                                    <tr>
                                        <td colSpan={4} className="table-empty-cell">
                                            {searchQuery ? (
                                                <div className="search-empty-content">
                                                    <Search size={24} style={{ opacity: 0.35 }} />
                                                    <p>{t("webFilter.noMatches", "No domains match your search.")}</p>
                                                    <button
                                                        type="button"
                                                        className="btn btn-secondary btn-sm"
                                                        onClick={() => { setSearchQuery(""); setCurrentPage(1); }}
                                                    >
                                                        {t("webFilter.clearSearch", "Clear search")}
                                                    </button>
                                                </div>
                                            ) : (
                                                <div className="empty-content">
                                                    <Globe size={32} style={{ opacity: 0.35 }} />
                                                    <p>{t("webFilter.noCustomDomains", "No custom domains blocked.")}</p>
                                                    <span className="empty-hint">{t("webFilter.addAbove", "Use the form above to add domains to your blocklist.")}</span>
                                                </div>
                                            )}
                                        </td>
                                    </tr>
                                ) : (
                                    pageDomains.map(d => {
                                        const isAdult = isNsfw(d);
                                        return (
                                            <tr key={d} className="domain-row">
                                                <td className="td-domain">
                                                    <div className="domain-name-cell">
                                                        <div className="domain-icon-box">
                                                            <Globe size={14} />
                                                        </div>
                                                        <span className="domain-name-text font-mono" title={d}>{d}</span>
                                                    </div>
                                                </td>
                                                <td className="td-category">
                                                    <span className={`domain-category-pill ${isAdult ? 'pill--adult' : 'pill--custom'}`}>
                                                        {isAdult ? "Adult Content" : "Custom Block"}
                                                    </span>
                                                </td>
                                                <td className="td-status">
                                                    <span className="domain-status-badge">
                                                        <span className="status-dot-pulse" />
                                                        {t("webFilter.blocked", "Blocked")}
                                                    </span>
                                                </td>
                                                <td className="td-action">
                                                    <button 
                                                        className="domain-remove-action-btn" 
                                                        onClick={() => handleRemove(d)}
                                                        title={t("webFilter.removeDomain", { domain: d })}
                                                    >
                                                        <Trash2 size={13} />
                                                        <span>{t("webFilter.remove", "Remove")}</span>
                                                    </button>
                                                </td>
                                            </tr>
                                        );
                                    })
                                )}
                            </tbody>
                        </table>
                    </div>

                    {(totalPages > 1 || (!showAdult && nsfwCount > 0)) && (
                        <div className="web-filter-table-footer">
                            <div className="footer-left">
                                {!showAdult && nsfwCount > 0 && (
                                    <button 
                                        type="button" 
                                        className="adult-toggle-btn" 
                                        onClick={handleShowAdult}
                                    >
                                        <EyeOff size={13} />
                                        <span>{t("webFilter.hidden", { count: nsfwCount })}</span>
                                    </button>
                                )}
                            </div>

                            {totalPages > 1 && (
                                <div className="pagination-controls">
                                    <span className="pagination-info">
                                        {startIndex + 1}–{Math.min(startIndex + PAGE_SIZE, filteredDomains.length)} of {filteredDomains.length}
                                    </span>
                                    <div className="pagination-buttons">
                                        <button
                                            type="button"
                                            className="pagination-btn"
                                            disabled={safePage <= 1}
                                            onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
                                            title="Previous page"
                                        >
                                            <ChevronLeft size={14} />
                                        </button>
                                        <span className="pagination-page-indicator">
                                            {safePage} / {totalPages}
                                        </span>
                                        <button
                                            type="button"
                                            className="pagination-btn"
                                            disabled={safePage >= totalPages}
                                            onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                                            title="Next page"
                                        >
                                            <ChevronRight size={14} />
                                        </button>
                                    </div>
                                </div>
                            )}
                        </div>
                    )}
                </div>
            </section>
        </div>
    );
}
