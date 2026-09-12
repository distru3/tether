import { useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { ShieldAlert, EyeOff, Globe, Sparkles, Upload, Plus, Trash2, Check, AlertCircle } from "lucide-react";
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
                                <Upload size={15} />
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
                    <div className="ledger-table-wrapper">
                        <table className="ledger-table">
                            <thead>
                                <tr>
                                    <th>{t("webFilter.domain", "Domain")}</th>
                                    <th>{t("webFilter.category", "Category")}</th>
                                    <th>{t("webFilter.status", "Status")}</th>
                                    <th style={{ textAlign: "right" }}>{t("webFilter.action", "Action")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {loading && !isUploading ? (
                                    <tr>
                                        <td colSpan={4} className="text-center" style={{ color: 'var(--text-muted)' }}>{t("common.loading", "Loading...")}</td>
                                    </tr>
                                ) : visibleDomains.length === 0 ? (
                                    <tr>
                                        <td colSpan={4} className="text-center" style={{ color: 'var(--text-muted)', padding: '32px 0' }}>
                                            <Globe size={28} style={{ opacity: 0.35, marginBottom: 8, display: 'block', margin: '0 auto 8px' }} />
                                            {t("webFilter.noCustomDomains", "No custom domains blocked.")}
                                        </td>
                                    </tr>
                                ) : (
                                    visibleDomains.map(d => {
                                        const isAdult = isNsfw(d);
                                        return (
                                            <tr key={d}>
                                                <td>
                                                    <div className="ledger-app-cell">
                                                        <div className="ledger-icon-box" style={{ backgroundColor: 'rgba(220, 160, 109, 0.15)', color: 'var(--color-accent)' }}>
                                                            <Globe size={15} />
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
                                                <td style={{ textAlign: "right" }}>
                                                    <button 
                                                        className="btn btn-ghost btn-sm text-danger" 
                                                        onClick={() => handleRemove(d)}
                                                        style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
                                                    >
                                                        <Trash2 size={13} />
                                                        {t("webFilter.remove", "Remove")}
                                                    </button>
                                                </td>
                                            </tr>
                                        );
                                    })
                                )}
                                {!showAdult && nsfwCount > 0 && (
                                    <tr>
                                        <td colSpan={4} style={{ textAlign: "center", padding: '12px 0' }}>
                                            <button 
                                                type="button" 
                                                className="btn btn-ghost btn-sm" 
                                                onClick={handleShowAdult}
                                                style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}
                                            >
                                                <EyeOff size={14} />
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
