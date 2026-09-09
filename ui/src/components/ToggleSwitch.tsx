import React from 'react';
import './ToggleSwitch.css';

interface ToggleSwitchProps {
    checked: boolean;
    onChange: (checked: boolean) => void;
    disabled?: boolean;
    id?: string;
}

export function ToggleSwitch({ checked, onChange, disabled, id }: ToggleSwitchProps) {
    return (
        <button
            type="button"
            id={id}
            role="switch"
            aria-checked={checked}
            disabled={disabled}
            className={`toggle-switch ${checked ? 'toggle-active' : ''}`}
            onClick={() => onChange(!checked)}
        >
            <span className="toggle-thumb" />
        </button>
    );
}
