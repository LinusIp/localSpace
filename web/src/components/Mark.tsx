// The mark: a ring and a dot on the accent, drawn in place.

export function Mark({ size = 28 }: { size?: number }) {
  const ring = size * 0.6;
  const dot = size * 0.2;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: size,
        height: size,
        borderRadius: 8,
        background: "var(--ls-accent-soft)",
      }}
    >
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: ring,
          height: ring,
          borderRadius: 999,
          border: "2px solid var(--ls-accent)",
          boxSizing: "border-box",
        }}
      >
        <span style={{ width: dot, height: dot, borderRadius: 999, background: "var(--ls-accent)" }} />
      </span>
    </span>
  );
}
