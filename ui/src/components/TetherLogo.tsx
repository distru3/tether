import React from "react";

interface TetherLogoProps {
  size?: number;
  className?: string;
  style?: React.CSSProperties;
}

export function TetherLogo({ size = 24, className = "", style }: TetherLogoProps) {
  return (
    <img
      src="/tether-logo.png"
      alt="Tether"
      width={size}
      height={size}
      className={`tether-brand-logo ${className}`}
      style={{
        width: size,
        height: size,
        objectFit: "contain",
        borderRadius: size > 24 ? 9 : 4,
        display: "block",
        userSelect: "none",
        flexShrink: 0,
        ...style,
      }}
      draggable={false}
    />
  );
}
