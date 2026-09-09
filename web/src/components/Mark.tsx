export function Mark({ size = 28 }: { size?: number }) {
  return (
    <span
      className="inline-flex items-center justify-center rounded-lg bg-accent-soft"
      style={{ width: size, height: size }}
    >
      <span
        className="inline-flex items-center justify-center rounded-full border-2 border-accent"
        style={{ width: size * 0.6, height: size * 0.6 }}
      >
        <span className="rounded-full bg-accent" style={{ width: size * 0.2, height: size * 0.2 }} />
      </span>
    </span>
  );
}
