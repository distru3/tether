import React from 'react';
import './MetricCards.css';

export interface MetricData {
    label: string;
    value: React.ReactNode;
    icon: React.ReactNode;
    badge?: string;
    trend?: string;
}

interface MetricCardsProps {
    metrics: MetricData[];
}

export function MetricCards({ metrics }: MetricCardsProps) {
    return (
        <div className="metric-cards-grid">
            {metrics.map((metric, idx) => (
                <div key={idx} className="metric-card">
                    <div className="metric-card-header">
                        <span className="metric-label">{metric.label}</span>
                        <div className="metric-icon">{metric.icon}</div>
                    </div>
                    <div className="metric-value-row">
                        <span className="metric-value">{metric.value}</span>
                        {metric.badge && <span className="metric-badge">{metric.badge}</span>}
                    </div>
                    {metric.trend && (
                        <div className="metric-footer">
                            <span className="metric-trend">{metric.trend}</span>
                        </div>
                    )}
                </div>
            ))}
        </div>
    );
}
