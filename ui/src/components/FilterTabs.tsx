import React from 'react';
import './FilterTabs.css';

interface FilterTabsProps {
    tabs: string[];
    activeTab: string;
    onChange: (tab: string) => void;
}

export function FilterTabs({ tabs, activeTab, onChange }: FilterTabsProps) {
    return (
        <div className="filter-tabs">
            {tabs.map((tab) => (
                <button
                    key={tab}
                    className={`filter-tab ${tab === activeTab ? 'active' : ''}`}
                    onClick={() => onChange(tab)}
                >
                    {tab}
                </button>
            ))}
        </div>
    );
}
