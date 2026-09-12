import React from "react";
import "./LoadingSpinner.css";

export interface LoadingSpinnerProps {
    size?: "xs" | "sm" | "md" | "lg";
    color?: string;
    className?: string;
    label?: string;
}

export const LoadingSpinner: React.FC<LoadingSpinnerProps> = ({
    size = "sm",
    color,
    className = "",
    label,
}) => {
    const sizePx = size === "xs" ? 14 : size === "sm" ? 18 : size === "md" ? 24 : 36;
    const strokeWidth = size === "xs" ? 3 : size === "sm" ? 2.5 : 2.5;

    return (
        <span
            className={`st-loading-spinner-wrapper st-loading-spinner--${size} ${className}`}
            role="status"
            aria-label={label ?? "Loading"}
        >
            <svg
                className="st-loading-spinner"
                width={sizePx}
                height={sizePx}
                viewBox="0 0 24 24"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
                style={{ color: color || "inherit" }}
            >
                <circle
                    className="st-spinner-track"
                    cx="12"
                    cy="12"
                    r="9.5"
                    stroke="currentColor"
                    strokeWidth={strokeWidth}
                    strokeOpacity="0.2"
                />
                <circle
                    className="st-spinner-head"
                    cx="12"
                    cy="12"
                    r="9.5"
                    stroke="currentColor"
                    strokeWidth={strokeWidth}
                    strokeLinecap="round"
                    strokeDasharray="40 60"
                />
            </svg>
            {label && <span className="st-spinner-label">{label}</span>}
        </span>
    );
};
