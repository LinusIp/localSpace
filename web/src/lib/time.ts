// Times as people read them.

export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} days ago`;
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** "Good morning", by the clock on the wall where the person sits. */
export function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 5) return "Good evening";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

/** "Today, 09:14", "Yesterday, 17:02", "3 days ago": when someone was last here. */
export function whenLabel(ms: number | null): string {
  if (ms === null) return "Never";
  const then = new Date(ms);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const time = then.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (ms >= startOfToday) return `Today, ${time}`;
  if (ms >= startOfToday - 86_400_000) return `Yesterday, ${time}`;
  const days = Math.round((startOfToday - ms) / 86_400_000);
  if (days < 30) return `${days} days ago`;
  return then.toLocaleDateString([], { year: "numeric", month: "short", day: "numeric" });
}
