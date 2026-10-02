import './FilterTabs.css';

interface FilterTabsProps {
    /** Stable keys; also the labels unless `labels` maps them. */
    tabs: string[];
    activeTab: string;
    onChange: (tab: string) => void;
    /** Display text per key, so keys stay stable across languages. */
    labels?: Record<string, string>;
    ariaLabel?: string;
}

export function FilterTabs({ tabs, activeTab, onChange, labels, ariaLabel }: FilterTabsProps) {
    return (
        <div className="filter-tabs" role="tablist" aria-label={ariaLabel}>
            {tabs.map((tab) => (
                <button
                    key={tab}
                    type="button"
                    role="tab"
                    aria-selected={tab === activeTab}
                    className={`filter-tab ${tab === activeTab ? 'active' : ''}`}
                    onClick={() => onChange(tab)}
                >
                    {labels?.[tab] ?? tab}
                </button>
            ))}
        </div>
    );
}
