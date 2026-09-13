import React from 'react';

interface ToggleSwitchProps {
    checked: boolean;
    onChange: (checked: boolean) => void;
    disabled?: boolean;
    id?: string;
}

export function ToggleSwitch({ checked, onChange, disabled, id }: ToggleSwitchProps) {
    return (
        <input
            type="checkbox"
            id={id}
            role="switch"
            aria-checked={checked}
            checked={checked}
            disabled={disabled}
            className="toggle-switch"
            onChange={(e) => onChange(e.target.checked)}
        />
    );
}
