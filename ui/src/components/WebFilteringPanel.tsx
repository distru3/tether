import { useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { addManualBlock, listManualBlocks, removeManualBlock } from "../api";
import { MetricCards } from "./MetricCards";
import "./WebFilteringPanel.css";

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
        onAttempt(t("pinGate.viewHidden"), async (pin: string) => {
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
                // Quick validation to ensure it looks somewhat like a domain
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
    const visibleDomains = domains.filter(d => showAdult || !isNsfw(d));

    const metrics = [
        { label: t("webFilter.totalBlocked", "Total Blocked"), value: domains.length, icon: <div style={{width: 16, height: 16, borderRadius: '50%', border: '2px solid currentColor'}} /> },
        { label: t("webFilter.hiddenDomains", "Hidden Domains"), value: nsfwCount, icon: <div style={{width: 16, height: 16, borderRadius: '2px', border: '2px solid currentColor', borderStyle: 'dashed'}} /> },
    ];

    return (
        <div className="view-container">
            <div className="view-header">
                <div>
                    <h2 className="view-title">{t("webFilter.customBlockedDomains", "Web Shield")}</h2>
                    <p className="view-subtitle">{t("webFilter.desc", "Manage blocked domains and bulk upload custom lists.")}</p>
                </div>
            </div>

            <MetricCards metrics={metrics} />

            <section className="card limits-panel">
                <div className="card-body">
                    {error && (
                        <div className="error-text" style={{ marginBottom: "16px", color: "var(--color-danger)", fontSize: "13px", display: "flex", alignItems: "center", gap: "6px" }}>
                            {error}
                        </div>
                    )}

                    <div className="web-filter-controls" style={{ display: "flex", gap: "16px", marginBottom: "24px", flexWrap: "wrap", alignItems: 'flex-start' }}>
                        <form onSubmit={handleAdd} style={{ display: "flex", gap: "8px", flex: "1", minWidth: "250px" }}>
                            <input
                                type="text"
                                value={newDomain}
                                onChange={e => setNewDomain(e.target.value)}
                                placeholder={t("webFilter.placeholder", "example.com")}
                                className="form-input"
                                style={{ flex: 1 }}
                            />
                            <button type="submit" className="btn btn-primary" disabled={!newDomain.trim()} style={{ whiteSpace: "nowrap" }}>
                                {t("webFilter.blockDomain", "Block Domain")}
                            </button>
                        </form>

                        <div className="bulk-upload-section" style={{ display: "flex", gap: "8px", alignItems: "center" }}>
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
                                {isUploading ? t("webFilter.importing", "Importing...") : t("webFilter.bulkUpload", "Bulk Upload")}
                            </button>
                        </div>
                    </div>

                    <div className="ai-prompt-card glass-card" style={{ padding: "16px", marginBottom: "24px", borderRadius: "8px", border: "1px dashed var(--border-subtle)", backgroundColor: "var(--bg-recessed)" }}>
                        <h3 style={{ margin: "0 0 8px 0", fontSize: "14px", fontWeight: 600 }}>{t("webFilter.generateListsWithAi", "Generate Blocklists with AI")}</h3>
                        <p style={{ margin: "0 0 12px 0", fontSize: "13px", color: "var(--text-muted)" }}>
                            {t("webFilter.aiDesc", "Enter a topic to copy an AI prompt that will generate a formatted domain list for bulk upload.")}
                        </p>
                        <div style={{ display: "flex", gap: "8px" }}>
                            <input
                                type="text"
                                value={aiCategory}
                                onChange={e => setAiCategory(e.target.value)}
                                placeholder={t("webFilter.aiPlaceholder", "e.g. news sites, video streaming...")}
                                className="form-input form-input-sm"
                                style={{ flex: 1 }}
                            />
                            <button 
                                type="button" 
                                className="btn btn-secondary btn-sm" style={{ whiteSpace: "nowrap" }} 
                                onClick={handleCopyPrompt}
                                disabled={!aiCategory.trim()}
                            >
                                {promptCopied ? t("webFilter.copied", "Copied!") : t("webFilter.copyPrompt", "Copy Prompt")}
                            </button>
                        </div>
                    </div>

                    <div className="ledger-table-wrapper" style={{ marginTop: 24 }}>
                        <table className="ledger-table">
                            <thead>
                                <tr>
                                    <th>{t("webFilter.domain", "Domain")}</th>
                                    <th>{t("webFilter.category", "Category")}</th>
                                    <th>{t("webFilter.status", "Status")}</th>
                                    <th>{t("webFilter.action", "Action")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {loading && !isUploading ? (
                                    <tr>
                                        <td colSpan={4} className="text-center" style={{ color: 'var(--text-muted)' }}>{t("common.loading", "Loading...")}</td>
                                    </tr>
                                ) : visibleDomains.length === 0 ? (
                                    <tr>
                                        <td colSpan={4} className="text-center" style={{ color: 'var(--text-muted)' }}>{t("webFilter.noCustomDomains", "No custom domains blocked.")}</td>
                                    </tr>
                                ) : (
                                    visibleDomains.map(d => {
                                        const isAdult = isNsfw(d);
                                        return (
                                            <tr key={d}>
                                                <td>
                                                    <div className="ledger-app-cell">
                                                        <div className="ledger-icon-box" style={{ backgroundColor: 'rgba(244, 63, 94, 0.1)', color: 'var(--color-danger)' }}>
                                                            <div style={{width: 14, height: 14, border: '2px solid currentColor', borderRadius: '50%'}} />
                                                        </div>
                                                        <span className="ledger-app-name font-mono">{d}</span>
                                                    </div>
                                                </td>
                                                <td>
                                                    <span className="ledger-category-pill">{isAdult ? "Adult Content" : "Custom Block"}</span>
                                                </td>
                                                <td>
                                                    <span className="badge badge-sm badge--danger">{t("webFilter.blocked", "Blocked")}</span>
                                                </td>
                                                <td>
                                                    <button 
                                                        className="btn btn-ghost btn-sm text-danger" 
                                                        onClick={() => handleRemove(d)}
                                                    >
                                                        {t("webFilter.remove", "Remove")}
                                                    </button>
                                                </td>
                                            </tr>
                                        );
                                    })
                                )}
                                {!showAdult && nsfwCount > 0 && (
                                    <tr>
                                        <td colSpan={4} style={{ textAlign: "center" }}>
                                            <button 
                                                type="button" 
                                                className="btn btn-ghost btn-sm" 
                                                onClick={handleShowAdult}
                                            >
                                                {t("webFilter.hidden", { count: nsfwCount })}
                                            </button>
                                        </td>
                                    </tr>
                                )}
                            </tbody>
                        </table>
                    </div>
                </div>
            </section>
        </div>
    );
}
