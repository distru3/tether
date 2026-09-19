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
        <div className="metric-telemetry-bar">
            {metrics.map((metric, idx) => (
                <div key={idx} className="metric-telemetry-item">
                    <div className="metric-telemetry-top">
                        <span className="metric-telemetry-label">{metric.label}</span>
                        <div className="metric-telemetry-icon">{metric.icon}</div>
                    </div>
                    <div className="metric-telemetry-val-row">
                        <span className="metric-telemetry-value">{metric.value}</span>
                        {metric.badge && <span className="metric-telemetry-badge">{metric.badge}</span>}
                    </div>
                    {metric.trend && (
                        <div className="metric-telemetry-trend">{metric.trend}</div>
                    )}
                </div>
            ))}
        </div>
    );
}
