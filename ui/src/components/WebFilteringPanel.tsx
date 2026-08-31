import { useEffect, useState } from "react";
import { addManualBlock, listManualBlocks, removeManualBlock } from "../api";
import "./WebFilteringPanel.css";

export function WebFilteringPanel() {
    const [domains, setDomains] = useState<string[]>([]);
    const [newDomain, setNewDomain] = useState("");
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

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
        const trimmed = newDomain.trim();
        if (!trimmed) return;

        setError(null);
        try {
            await addManualBlock(trimmed);
            setNewDomain("");
            refresh();
        } catch (e: any) {
            setError(e.toString());
        }
    };

    const handleRemove = async (domain: string) => {
        setError(null);
        try {
            await removeManualBlock(domain);
            refresh();
        } catch (e: any) {
            setError(e.toString());
        }
    };

    return (
        <section className="card limits-panel" style={{ maxWidth: "640px" }}>
            <header className="card-header">
                <h2>Custom Blocked Domains</h2>
                <div className="card-subtitle">
                    Domains blocked permanently on this device. (Uses the Windows hosts file - requires Service mode to bypass UAC).
                </div>
            </header>
            <div className="card-body">
                {error && <div className="error-banner">{error}</div>}

                <form className="inline-add-form" onSubmit={handleAdd}>
                    <input
                        type="text"
                        value={newDomain}
                        onChange={e => setNewDomain(e.target.value)}
                        placeholder="e.g. facebook.com"
                        className="form-input"
                    />
                    <button type="submit" className="btn btn-primary" disabled={!newDomain.trim()}>
                        Block Domain
                    </button>
                </form>

                {loading ? (
                    <div className="empty-state">Loading...</div>
                ) : domains.length === 0 ? (
                    <div className="empty-state">No custom domains blocked.</div>
                ) : (
                    <ul className="domain-list">
                        {domains.map(d => (
                            <li key={d} className="domain-item">
                                <span className="domain-name">{d}</span>
                                <button className="btn btn-ghost text-danger btn-sm" onClick={() => handleRemove(d)}>
                                    Remove
                                </button>
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        </section>
    );
}
