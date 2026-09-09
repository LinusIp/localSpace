// The left rail: the pages of the shell, and settings and help at the bottom.

import {
  Bot,
  Database,
  HelpCircle,
  History,
  Library,
  MessageSquare,
  Settings,
  Boxes,
  Wrench,
} from "lucide-react";
import type { Page } from "../store";
import { useSession } from "../store";
import { Mark } from "./Mark";

const ITEMS: Array<{ page: Page; label: string; icon: React.ComponentType<{ size?: number }> }> = [
  { page: "chat", label: "Chat", icon: MessageSquare },
  { page: "agents", label: "Agents", icon: Bot },
  { page: "tools", label: "Tools", icon: Wrench },
  { page: "models", label: "Models", icon: Boxes },
  { page: "data", label: "Data", icon: Database },
  { page: "history", label: "History", icon: History },
  { page: "library", label: "Library", icon: Library },
];

const BOTTOM: Array<{ page: Page; label: string; icon: React.ComponentType<{ size?: number }> }> = [
  { page: "settings", label: "Settings", icon: Settings },
  { page: "help", label: "Help", icon: HelpCircle },
];

export function Rail() {
  const page = useSession((s) => s.page);
  const go = useSession((s) => s.go);
  const Item = ({
    item,
  }: {
    item: { page: Page; label: string; icon: React.ComponentType<{ size?: number }> };
  }) => {
    const active = page === item.page;
    const Icon = item.icon;
    return (
      <button
        onClick={() => go(item.page)}
        className={`flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-[14px] transition-colors ${
          active ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-page hover:text-ink"
        }`}
      >
        <Icon size={18} />
        {item.label}
      </button>
    );
  };
  return (
    <nav className="flex w-52 shrink-0 flex-col border-r border-line bg-white px-3 py-4">
      <div className="mb-6 flex items-center gap-2.5 px-2">
        <Mark />
        <span className="text-[17px] font-semibold text-ink">localSpace</span>
      </div>
      <div className="flex flex-col gap-1">
        {ITEMS.map((item) => (
          <Item key={item.page} item={item} />
        ))}
      </div>
      <div className="mt-auto flex flex-col gap-1">
        {BOTTOM.map((item) => (
          <Item key={item.page} item={item} />
        ))}
      </div>
    </nav>
  );
}
