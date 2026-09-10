// The left rail: the pages of the shell, and settings and help at the bottom.

import type { ComponentType } from "react";
import {
  AgentIcon,
  ChatIcon,
  DataIcon,
  HelpIcon,
  HistoryIcon,
  LibraryIcon,
  ModelsIcon,
  SettingsIcon,
  ToolsIcon,
  type IconProps,
} from "@localspace/ui";
import type { Page } from "../store";
import { useSession } from "../store";
import { Mark } from "./Mark";

type Entry = { page: Page; label: string; icon: ComponentType<IconProps> };

const ITEMS: Entry[] = [
  { page: "chat", label: "Chat", icon: ChatIcon },
  { page: "agents", label: "Agents", icon: AgentIcon },
  { page: "tools", label: "Tools", icon: ToolsIcon },
  { page: "models", label: "Models", icon: ModelsIcon },
  { page: "data", label: "Data", icon: DataIcon },
  { page: "history", label: "History", icon: HistoryIcon },
  { page: "library", label: "Library", icon: LibraryIcon },
];

const BOTTOM: Entry[] = [
  { page: "settings", label: "Settings", icon: SettingsIcon },
  { page: "help", label: "Help", icon: HelpIcon },
];

// A top-level component, not one defined inside `Rail`: a component created
// on every render is a new type to React, which remounts its DOM each time
// and loses focus and accessibility handles with it.
function Item({ entry, active, go }: { entry: Entry; active: boolean; go: (page: Page) => void }) {
  const Icon = entry.icon;
  return (
    <button type="button" onClick={() => go(entry.page)} className={`ls-rail-item${active ? " ls-on" : ""}`} aria-current={active ? "page" : undefined}>
      <Icon size={18} />
      {entry.label}
    </button>
  );
}

export function Rail() {
  const page = useSession((s) => s.page);
  const go = useSession((s) => s.go);
  return (
    <nav className="ls-rail ls-border-r" style={{ background: "var(--ls-surface)", width: 208 }}>
      <div className="ls-row ls-gap-2 ls-mb-3" style={{ padding: "0 8px 16px" }}>
        <Mark />
        <span className="ls-strong" style={{ fontSize: 17 }}>
          localSpace
        </span>
      </div>
      {ITEMS.map((entry) => (
        <Item key={entry.page} entry={entry} active={page === entry.page} go={go} />
      ))}
      <div className="ls-rail-spacer" />
      {BOTTOM.map((entry) => (
        <Item key={entry.page} entry={entry} active={page === entry.page} go={go} />
      ))}
    </nav>
  );
}
