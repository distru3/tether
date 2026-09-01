import { useEffect, useState, useRef } from "react";
import { addManualBlock, listManualBlocks, removeManualBlock } from "../api";
import "./WebFilteringPanel.css";

export function WebFilteringPanel({ onAttempt }: { onAttempt: (label: string, run: (pin: string) => Promise<void>) => void }) {
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
            setError("This domain is already blocked.");
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
        onAttempt("View Hidden Domains", async (pin: string) => {
            setShowAdult(true);
        });
    };

    const isNsfw = (domain: string) => {
        return /(porn|xvideo|xnxx|xhamster|chaturbate|stripchat|onlyfans|redtube|tubegalore|eporner|spankbang|rule34|xxx)/i.test(domain);
    };

    const handleRemove = (domain: string) => {
        setError(null);
        onAttempt(`Remove block on ${domain}`, async (pin: string) => {
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
                throw new Error("All domains in this file are already blocked.");
            }

            if (uniqueNew.length > 100) {
                throw new Error(`File contains ${uniqueNew.length} new domains. Please limit to 100 maximum.`);
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

    return (
        <section className="card limits-panel" style={{ maxWidth: "800px" }}>
            <header className="card-header" style={{ display: "block" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <h2 style={{ margin: 0, whiteSpace: "nowrap" }}>Custom Blocked Domains</h2>
                    <span className="badge badge-sm badge--warning">Experimental</span>
                </div>
                <div className="card-subtitle" style={{ marginTop: "8px" }}>
                    Domains blocked permanently on this device using the Windows hosts file.
                    <div style={{ marginTop: "8px", fontSize: "12px", color: "var(--text-muted)", display: "flex", gap: "6px" }}>
                        <span>ℹ️</span>
                        <span>Due to browser caching and "Secure DNS" bypasses, some domains might not get blocked immediately (or at all). You may need to manually disable Secure DNS in your browser settings or fully restart your browser for changes to take effect.</span>
                    </div>
                </div>
            </header>
            
            <div className="card-body">
                {error && <div className="error-text" style={{ marginBottom: "16px", color: "var(--color-danger)", fontSize: "13px", display: "flex", alignItems: "center", gap: "6px" }}>⚠️ {error}</div>}

                <div className="web-filter-controls" style={{ display: "flex", gap: "16px", marginBottom: "24px", flexWrap: "wrap" }}>
                    <form onSubmit={handleAdd} style={{ display: "flex", gap: "8px", flex: "1", minWidth: "250px", alignItems: "flex-start" }}>
                        <input
                            type="text"
                            value={newDomain}
                            onChange={e => setNewDomain(e.target.value)}
                            placeholder="e.g. facebook.com"
                            className="form-input"
                            style={{ flex: 1 }}
                        />
                        <button type="submit" className="btn btn--primary" disabled={!newDomain.trim()} style={{ whiteSpace: "nowrap" }}>
                            Block Domain
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
                            className="btn btn--secondary" 
                            onClick={() => fileInputRef.current?.click()}
                            disabled={isUploading}
                        >
                            {isUploading ? "Importing..." : "Bulk Upload (.txt)"}
                        </button>
                    </div>
                </div>

                <div className="ai-prompt-card glass-card" style={{ padding: "16px", marginBottom: "24px", borderRadius: "8px", border: "1px dashed var(--border-light)" }}>
                    <h3 style={{ margin: "0 0 8px 0", fontSize: "14px" }}>Generate Lists with AI</h3>
                    <p style={{ margin: "0 0 12px 0", fontSize: "13px", color: "var(--text-muted)" }}>
                        Want to block an entire category? Enter a topic below and copy a specialized prompt to feed into ChatGPT or Claude. Save the AI's response as a .txt file and upload it above.
                    </p>
                    <div style={{ display: "flex", gap: "8px" }}>
                        <input
                            type="text"
                            value={aiCategory}
                            onChange={e => setAiCategory(e.target.value)}
                            placeholder="e.g. Social Media, Adult Content, News"
                            className="form-input form-input-sm"
                            style={{ flex: 1 }}
                        />
                        <button 
                            type="button" 
                            className="btn btn--secondary btn-sm" style={{ whiteSpace: "nowrap" }} 
                            onClick={handleCopyPrompt}
                            disabled={!aiCategory.trim()}
                        >
                            {promptCopied ? "Copied!" : "Copy AI Prompt"}
                        </button>
                    </div>
                </div>

                {loading && !isUploading ? (
                    <div className="empty-state">Loading...</div>
                ) : domains.length === 0 ? (
                    <div className="empty-state">No custom domains blocked.</div>
                ) : (
                    <div className="domain-chip-container" style={{ display: "flex", flexWrap: "wrap", gap: "8px", maxHeight: "300px", overflowY: "auto", paddingRight: "8px" }}>
                        {domains.filter(d => showAdult || !isNsfw(d)).map(d => (
                            <div key={d} className="domain-chip" style={{ display: "inline-flex", alignItems: "center", background: "var(--bg-surface-raised)", border: "1px solid var(--border-light)", borderRadius: "16px", padding: "4px 10px", fontSize: "13px", gap: "6px" }}>
                                <span className="domain-name" style={{ fontFamily: "var(--font-mono)" }}>{d}</span>
                                <button 
                                    className="domain-remove-btn" 
                                    onClick={() => handleRemove(d)}
                                    title="Remove block"
                                    style={{ background: "transparent", border: "none", color: "var(--text-muted)", cursor: "pointer", fontSize: "16px", lineHeight: 1, padding: "0 4px", display: "flex", alignItems: "center" }}
                                >
                                    &times;
                                </button>
                            </div>
                        ))}
                        
                        {!showAdult && domains.some(isNsfw) && (
                            <button 
                                type="button" 
                                className="domain-chip" 
                                onClick={handleShowAdult}
                                title="Unlock hidden domains"
                                style={{ 
                                    background: "transparent", 
                                    border: "1px dashed var(--border-light)", 
                                    color: "var(--text-muted)", 
                                    cursor: "pointer", 
                                    padding: "4px 12px",
                                    fontSize: "12px",
                                    display: "inline-flex",
                                    alignItems: "center",
                                    borderRadius: "16px",
                                    transition: "all 0.2s ease"
                                }}
                                onMouseEnter={(e) => { 
                                    e.currentTarget.style.color = "var(--text-primary)"; 
                                    e.currentTarget.style.borderColor = "var(--text-muted)"; 
                                }}
                                onMouseLeave={(e) => { 
                                    e.currentTarget.style.color = "var(--text-muted)"; 
                                    e.currentTarget.style.borderColor = "var(--border-light)"; 
                                }}
                            >
                                <span>{domains.filter(isNsfw).length} hidden</span>
                            </button>
                        )}
                    </div>
                )}
            </div>
        </section>
    );
}
