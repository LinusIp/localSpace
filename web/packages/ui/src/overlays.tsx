// Menus, dialogs, toasts and tabs: what floats over the page or sits on top
// of a panel. Keyboard and pointer both close what they opened.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { CloseIcon } from "./icons.tsx";
import { cx } from "./controls.tsx";

export interface MenuItem {
  id: string;
  label: ReactNode;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

/** A button that opens a menu beneath it; Escape and a click outside close it. */
export function Menu({ trigger, items, align = "left" }: { trigger: (open: () => void, isOpen: boolean) => ReactNode; items: MenuItem[]; align?: "left" | "right" }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <div ref={root} className="ls-relative" style={{ display: "inline-block" }}>
      {trigger(() => setOpen((v) => !v), open)}
      {open && (
        <div className="ls-menu" role="menu" style={align === "right" ? { right: 0, top: "100%", marginTop: 4 } : { left: 0, top: "100%", marginTop: 4 }}>
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              disabled={item.disabled}
              className={cx("ls-menu-item", item.danger && "ls-danger")}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Buttons for the bottom right; the dialog closes on Escape and the backdrop. */
  actions?: ReactNode;
}

export function Dialog({ open, title, onClose, children, actions }: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="ls-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="ls-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <div className="ls-row ls-between ls-mb-3">
          <h2 style={{ margin: 0 }}>{title}</h2>
          <button type="button" className="ls-iconbutton ls-quiet" aria-label="Close" onClick={onClose}>
            <CloseIcon />
          </button>
        </div>
        {children}
        {actions && <div className="ls-dialog-actions">{actions}</div>}
      </div>
    </div>
  );
}

/** A short message at the bottom of the page for a few seconds. */
export function Toast({ level, children }: { level: "info" | "warn" | "error"; children: ReactNode }) {
  return <div className={cx("ls-toast", level === "warn" && "ls-warn-toast", level === "error" && "ls-danger-toast")}>{children}</div>;
}

export interface Tab {
  id: string;
  label: ReactNode;
  title?: string;
  closable?: boolean;
}

export function Tabs({ tabs, active, onSelect, onClose, trailing }: { tabs: Tab[]; active: string | null; onSelect: (id: string) => void; onClose?: (id: string) => void; trailing?: ReactNode }) {
  return (
    <div className="ls-tabs" role="tablist">
      {tabs.map((t) => (
        <div key={t.id} className={cx("ls-tab", t.id === active && "ls-on")} role="tab" aria-selected={t.id === active} title={t.title}>
          <button type="button" className="ls-link-button" style={{ color: "inherit", fontSize: 14 }} onClick={() => onSelect(t.id)}>
            {t.label}
          </button>
          {t.closable && onClose && (
            <button type="button" className="ls-tab-close" aria-label={`Close ${typeof t.label === "string" ? t.label : t.id}`} onClick={() => onClose(t.id)}>
              <CloseIcon size={13} />
            </button>
          )}
        </div>
      ))}
      {trailing && <span className="ls-ml-auto ls-row">{trailing}</span>}
    </div>
  );
}
