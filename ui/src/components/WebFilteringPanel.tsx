import { useEffect, useMemo, useRef, useState, type ChangeEvent, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronLeft, ChevronRight, Search, Trash2, Upload, X } from "lucide-react";
import { addManualBlock, describeError, listManualBlocks, removeManualBlock, verifyPin } from "../api";
import { isHiddenDomain } from "../domains";
import { LoadingSpinner } from "./LoadingSpinner";

/** Thrown `Error`s carry our own (already translated) text; invoke failures are wire errors. */
function messageOf(e: unknown): string {
    return e instanceof Error ? e.message : describeError(e);
}

/**
 * The domain on one line of an uploaded list. Accepts plain domains and
 * hosts-file lines (`0.0.0.0 example.com`), which is how most published
 * blocklists are distributed; strips trailing comments.
 */
function domainFromLine(line: string): string | null {
    const content = (line.split("#")[0] ?? "").trim().toLowerCase();
    const tokens = content.split(/[\s,]+/).filter(Boolean);
    const first = tokens[0];
    if (first === undefined) return null;
    const candidate = /^[\d.:]+$/.test(first) ? tokens[1] : first;
    return candidate !== undefined && candidate.includes(".") ? candidate : null;
}

const PAGE_SIZE = 10;
/** Largest list one import may add. */
const IMPORT_MAX = 100;

/**
 * The Websites subview of Limits: add or import sites, and the list of
 * blocked sites. Sites from the built-in adult list are hidden until the PIN
 * is entered, and counted separately so the numbers always add up.
 */
export function WebFilteringPanel({ onAttempt }: { onAttempt: (label: string, run: (pin: string) => Promise<void>) => void }) {
    const { t, i18n } = useTranslation();
    const [domains, setDomains] = useState<string[]>([]);
    const [draft, setDraft] = useState("");
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [topic, setTopic] = useState("");
    const [copied, setCopied] = useState(false);
    const [importing, setImporting] = useState(false);
    const [showHidden, setShowHidden] = useState(false);
    const [query, setQuery] = useState("");
    const [page, setPage] = useState(1);
    const fileInput = useRef<HTMLInputElement>(null);

    const refresh = () => {
        setLoading(true);
        listManualBlocks()
            .then((res) => setDomains(res.domains))
            .catch((e) => setError(messageOf(e)))
            .finally(() => setLoading(false));
    };

    useEffect(refresh, []);

    const add = async (e: FormEvent) => {
        e.preventDefault();
        const domain = draft.trim().toLowerCase();
        if (!domain) return;
        if (domains.some((d) => d.toLowerCase() === domain)) {
            setError(t("webFilter.alreadyBlocked"));
            return;
        }
        setError(null);
        try {
            await addManualBlock(domain);
            setDraft("");
            refresh();
        } catch (err) {
            setError(messageOf(err));
        }
    };

    const remove = (domain: string) => {
        setError(null);
        onAttempt(t("pinGate.removeBlock", { domain }), async (pin) => {
            await removeManualBlock(domain, pin);
            refresh();
        });
    };

    const revealHidden = () => {
        onAttempt(t("pinGate.viewHidden"), async (pin) => {
            // Ask the agent: a UI-only check would accept any input.
            await verifyPin(pin);
            setShowHidden(true);
        });
    };

    const copyPrompt = () => {
        if (!topic.trim()) return;
        const prompt = `Please generate a plain text file containing a list of domains related to ${topic.trim()}. Provide ONLY the raw domain names, one per line, with no extra text, numbering, or formatting (e.g. 'example.com'). Do not include subdomains like 'www.' unless necessary. The list MUST NOT exceed ${IMPORT_MAX} domains.`;
        void navigator.clipboard.writeText(prompt).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
        });
    };

    const importFile = async (e: ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        setError(null);
        setImporting(true);
        try {
            const parsed = (await file.text())
                .split(/[\r\n]+/)
                .map(domainFromLine)
                .filter((d): d is string => d !== null);
            const existing = new Set(domains.map((d) => d.toLowerCase()));
            const fresh = Array.from(new Set(parsed)).filter((d) => !existing.has(d));
            if (fresh.length === 0) throw new Error(t("webFilter.allBlocked"));
            if (fresh.length > IMPORT_MAX) throw new Error(t("webFilter.tooMany", { count: fresh.length, max: IMPORT_MAX }));
            // One bad line must not abort the rest of the import.
            let added = 0;
            let skipped = 0;
            for (const domain of fresh) {
                try {
                    await addManualBlock(domain);
                    added += 1;
                } catch {
                    skipped += 1;
                }
            }
            refresh();
            if (skipped > 0) setError(t("webFilter.importSummary", { added, skipped }));
        } catch (err) {
            setError(messageOf(err));
        } finally {
            setImporting(false);
            if (fileInput.current) fileInput.current.value = "";
        }
    };

    const hiddenCount = domains.filter(isHiddenDomain).length;
    const visible = useMemo(() => domains.filter((d) => showHidden || !isHiddenDomain(d)), [domains, showHidden]);
    const matches = useMemo(() => {
        const q = query.trim().toLowerCase();
        return q ? visible.filter((d) => d.toLowerCase().includes(q)) : visible;
    }, [visible, query]);

    const pages = Math.max(1, Math.ceil(matches.length / PAGE_SIZE));
    const current = Math.min(page, pages);
    const first = (current - 1) * PAGE_SIZE;
    const shown = matches.slice(first, first + PAGE_SIZE);
    const number = (n: number) => n.toLocaleString(i18n.language);

    return (
        <>
            <div className="tt-head">
                <div>
                    <h1 className="tt-title">{t("webFilter.customBlockedDomains")}</h1>
                    <p className="tt-sub">{t("webFilter.desc")}</p>
                </div>
            </div>

            <section className="tt-card" aria-labelledby="web-add">
                <h2 id="web-add" className="tt-card-title">{t("webFilter.addTitle")}</h2>
                <form className="tt-inline-form" onSubmit={(e) => void add(e)}>
                    <label htmlFor="web-site" className="tt-sr-only">{t("limitsPage.siteLabel")}</label>
                    <input
                        id="web-site"
                        className="tt-input"
                        value={draft}
                        onChange={(e) => setDraft(e.target.value)}
                        placeholder={t("limitsPage.sitePlaceholder")}
                        autoComplete="off"
                        spellCheck={false}
                    />
                    <button type="submit" className="tt-btn tt-btn--primary tt-btn--sm">{t("limitsPage.block")}</button>
                </form>
                <div className="tt-web-import">
                    <input type="file" accept=".txt,.csv" ref={fileInput} hidden onChange={(e) => void importFile(e)} />
                    <button
                        type="button"
                        className="tt-btn tt-btn--outline tt-btn--sm"
                        onClick={() => fileInput.current?.click()}
                        disabled={importing}
                    >
                        {importing ? <LoadingSpinner size="xs" /> : <Upload size={16} aria-hidden="true" />}
                        {importing ? t("webFilter.importing") : t("webFilter.bulkUpload")}
                    </button>
                    <span className="tt-row-detail">{t("webFilter.importHint", { max: IMPORT_MAX })}</span>
                </div>
                {error !== null && <p className="tt-error" role="alert">{error}</p>}

                <details className="tt-details">
                    <summary>{t("webFilter.generateListsWithAi")}</summary>
                    <p className="tt-sub">{t("webFilter.aiDesc")}</p>
                    <div className="tt-inline-form">
                        <label htmlFor="web-topic" className="tt-sr-only">{t("webFilter.aiTopic")}</label>
                        <input
                            id="web-topic"
                            className="tt-input"
                            value={topic}
                            onChange={(e) => setTopic(e.target.value)}
                            placeholder={t("webFilter.aiPlaceholder")}
                        />
                        <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" onClick={copyPrompt} disabled={!topic.trim()}>
                            {copied && <Check size={16} aria-hidden="true" />}
                            {copied ? t("webFilter.copied") : t("webFilter.copyPrompt")}
                        </button>
                    </div>
                </details>
            </section>

            <section className="tt-card" aria-labelledby="web-list">
                <div className="tt-card-head">
                    <h2 id="web-list" className="tt-card-title">
                        {t("webFilter.activeRules")} <span className="tt-tag">{number(visible.length)}</span>
                    </h2>
                    <span className="tt-search">
                        <Search size={16} aria-hidden="true" />
                        <input
                            className="tt-input"
                            type="search"
                            value={query}
                            onChange={(e) => {
                                setQuery(e.target.value);
                                setPage(1);
                            }}
                            placeholder={t("webFilter.searchPlaceholder")}
                            aria-label={t("webFilter.searchPlaceholder")}
                        />
                    </span>
                </div>

                {loading && domains.length === 0 ? (
                    <LoadingSpinner size="md" />
                ) : matches.length === 0 ? (
                    <p className="tt-sub">
                        {query ? t("webFilter.noMatches") : t("webFilter.noCustomDomains")}
                        {query && (
                            <>
                                {" "}
                                <button type="button" className="tt-link" onClick={() => setQuery("")}>
                                    {t("webFilter.clearSearch")}
                                </button>
                            </>
                        )}
                    </p>
                ) : (
                    <ul className="tt-list">
                        {shown.map((d) => (
                            <li className="tt-row" key={d}>
                                <span className="tt-row-text">
                                    <span className="tt-domain" title={d}>
                                        {d}
                                        {isHiddenDomain(d) && <span className="tt-tag">{t("webFilter.kindAdult")}</span>}
                                    </span>
                                </span>
                                <button
                                    type="button"
                                    className="tt-icon-btn"
                                    onClick={() => remove(d)}
                                    aria-label={t("webFilter.removeDomain", { domain: d })}
                                    title={t("webFilter.removeDomain", { domain: d })}
                                >
                                    <Trash2 size={16} aria-hidden="true" />
                                </button>
                            </li>
                        ))}
                    </ul>
                )}

                {(pages > 1 || (!showHidden && hiddenCount > 0)) && (
                    <div className="tt-web-footer">
                        {!showHidden && hiddenCount > 0 ? (
                            <span className="tt-row-detail">
                                {t("webFilter.hiddenNote", { count: hiddenCount })}{" "}
                                <button type="button" className="tt-link" onClick={revealHidden}>
                                    {t("webFilter.showHidden")}
                                </button>
                            </span>
                        ) : (
                            <span />
                        )}
                        {pages > 1 && (
                            <span className="tt-pager">
                                <span className="tt-row-detail">
                                    {t("webFilter.range", {
                                        from: number(first + 1),
                                        to: number(Math.min(first + PAGE_SIZE, matches.length)),
                                        total: number(matches.length),
                                    })}
                                </span>
                                <button
                                    type="button"
                                    className="tt-icon-btn tt-icon-btn--plain"
                                    disabled={current <= 1}
                                    onClick={() => setPage(current - 1)}
                                    aria-label={t("webFilter.prevPage")}
                                >
                                    <ChevronLeft size={18} aria-hidden="true" className="tt-flip-rtl" />
                                </button>
                                <button
                                    type="button"
                                    className="tt-icon-btn tt-icon-btn--plain"
                                    disabled={current >= pages}
                                    onClick={() => setPage(current + 1)}
                                    aria-label={t("webFilter.nextPage")}
                                >
                                    <ChevronRight size={18} aria-hidden="true" className="tt-flip-rtl" />
                                </button>
                            </span>
                        )}
                    </div>
                )}
                {showHidden && hiddenCount > 0 && (
                    <button type="button" className="tt-link tt-web-hide" onClick={() => setShowHidden(false)}>
                        <X size={14} aria-hidden="true" />
                        {t("webFilter.hideHidden")}
                    </button>
                )}
            </section>
        </>
    );
}
