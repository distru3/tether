import React from 'react';
import './FrictionBanner.css';
import { ShieldAlert } from 'lucide-react';

interface FrictionBannerProps {
    title: string;
    description: string;
    primaryLabel: string;
    onPrimaryClick: () => void;
    secondaryLabel?: string;
    onSecondaryClick?: () => void;
}

export function FrictionBanner({ title, description, primaryLabel, onPrimaryClick, secondaryLabel, onSecondaryClick }: FrictionBannerProps) {
    return (
        <div className="friction-banner">
            <div className="friction-banner-content">
                <div className="friction-icon-box">
                    <ShieldAlert size={20} />
                </div>
                <div className="friction-text">
                    <div className="friction-title">{title}</div>
                    <div className="friction-desc">{description}</div>
                </div>
            </div>
            <div className="friction-actions">
                {secondaryLabel && onSecondaryClick && (
                    <button className="btn btn-secondary" onClick={onSecondaryClick}>
                        {secondaryLabel}
                    </button>
                )}
                <button className="btn btn-primary" onClick={onPrimaryClick}>
                    {primaryLabel}
                </button>
            </div>
        </div>
    );
}
