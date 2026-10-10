
interface ToggleSwitchProps {
    checked: boolean;
    onChange: (checked: boolean) => void;
    disabled?: boolean;
    id?: string;
    /** Accessible name; required unless a <label htmlFor={id}> names it. */
    label?: string;
}

export function ToggleSwitch({ checked, onChange, disabled, id, label }: ToggleSwitchProps) {
    return (
        <input
            type="checkbox"
            id={id}
            role="switch"
            aria-label={label}
            aria-checked={checked}
            checked={checked}
            disabled={disabled}
            className="toggle-switch"
            onChange={(e) => onChange(e.target.checked)}
        />
    );
}
