// The mark (web/public/brand/README.md): a pointy-top rounded hexagon with
// three nodes joined by connectors, drawn in place so it never blurs. Ink
// by default; any colour the caller names.

export function Mark({ size = 22, color = "#1C1E20" }: { size?: number; color?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 256 256" aria-hidden="true" focusable="false">
      <g fill="none" stroke={color} strokeLinecap="round" strokeLinejoin="round">
        <path
          d="M 136.40 40.85 L 199.27 77.15 A 16.80 16.80 0 0 1 207.67 91.70 L 207.67 164.30 A 16.80 16.80 0 0 1 199.27 178.85 L 136.40 215.15 A 16.80 16.80 0 0 1 119.60 215.15 L 56.73 178.85 A 16.80 16.80 0 0 1 48.33 164.30 L 48.33 91.70 A 16.80 16.80 0 0 1 56.73 77.15 L 119.60 40.85 A 16.80 16.80 0 0 1 136.40 40.85 Z"
          strokeWidth="23.3"
        />
        <path d="M 88.50 105.30 L 128.00 131.20 L 167.50 105.30" strokeWidth="9.1" />
        <path d="M 128.00 131.20 L 128.00 208.30" strokeWidth="9.1" />
      </g>
      <g fill={color}>
        <circle cx="88.5" cy="105.3" r="15.5" />
        <circle cx="167.5" cy="105.3" r="15.5" />
        <circle cx="128" cy="168.2" r="15.5" />
      </g>
    </svg>
  );
}

/** The mark with the name beside it, as the rail shows it. */
export function Brand({ size = 22, nameless }: { size?: number; nameless?: boolean }) {
  return (
    <span className="rail-brand">
      <Mark size={size} />
      {!nameless && <span>localSpace</span>}
    </span>
  );
}
