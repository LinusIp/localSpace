// Buttons, inputs, switches, pills: the controls the shell and the
// surfaces are made of. Function components over the stylesheet's classes;
// every prop the DOM element takes passes through.

import type { ButtonHTMLAttributes, CSSProperties, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from "react";
import { ChevronDownIcon, SpinnerIcon } from "./icons.tsx";

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

export type Tone = "neutral" | "ok" | "warn" | "danger";

const pillTone: Record<Tone, string> = { neutral: "", ok: "ls-ok", warn: "ls-warn-pill", danger: "ls-danger-pill" };

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  kind?: "ghost" | "primary" | "danger" | "quiet";
  size?: "normal" | "small";
  block?: boolean;
  busy?: boolean;
}

export function Button({ kind = "ghost", size = "normal", block, busy, className, children, disabled, type = "button", ...rest }: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled || busy}
      className={cx(
        "ls-button",
        kind === "primary" && "ls-primary",
        kind === "danger" && "ls-danger-button",
        kind === "quiet" && "ls-quiet",
        size === "small" && "ls-small-button",
        block && "ls-block",
        className,
      )}
      {...rest}
    >
      {busy && <SpinnerIcon className="ls-spin" size={14} />}
      {children}
    </button>
  );
}

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** The accessible name; also the tooltip. */
  label: string;
  quiet?: boolean;
  on?: boolean;
  round?: boolean;
}

export function IconButton({ label, quiet, on, round, className, children, type = "button", ...rest }: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      title={rest.title ?? label}
      aria-pressed={on === undefined ? undefined : on}
      className={cx("ls-iconbutton", quiet && "ls-quiet", on && "ls-on", round && "ls-round", className)}
      {...rest}
    >
      {children}
    </button>
  );
}

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  mono?: boolean;
}

export function Input({ mono, className, ...rest }: InputProps) {
  return <input className={cx("ls-input", mono && "ls-mono", className)} {...rest} />;
}

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  mono?: boolean;
}

export function Textarea({ mono, className, ...rest }: TextareaProps) {
  return <textarea className={cx("ls-textarea", mono && "ls-mono", className)} {...rest} />;
}

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  wrapClassName?: string;
}

export function Select({ className, wrapClassName, children, ...rest }: SelectProps) {
  return (
    <span className={cx("ls-selectwrap", wrapClassName)}>
      <select className={cx("ls-select", className)} {...rest}>
        {children}
      </select>
      <ChevronDownIcon size={14} />
    </span>
  );
}

export function Field({ label, children, className, htmlFor }: { label: ReactNode; children: ReactNode; className?: string; htmlFor?: string }) {
  return (
    <label className={cx("ls-field", className)} htmlFor={htmlFor}>
      <span className="ls-label">{label}</span>
      {children}
    </label>
  );
}

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className="ls-switch"
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

export function Pill({ tone = "neutral", children, className }: { tone?: Tone; children: ReactNode; className?: string }) {
  return <span className={cx("ls-pill", pillTone[tone], className)}>{children}</span>;
}

export function Dot({ tone }: { tone: Tone }) {
  return <span className={cx("ls-dot", pillTone[tone])} />;
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="ls-kbd">{children}</kbd>;
}

export function Card({ children, className, style }: { children: ReactNode; className?: string; style?: CSSProperties }) {
  return (
    <div className={cx("ls-card", className)} style={style}>
      {children}
    </div>
  );
}

export function SectionTitle({ children, action, onAction }: { children: ReactNode; action?: string; onAction?: () => void }) {
  return (
    <div className="ls-section-head">
      <h2 className="ls-section-title">{children}</h2>
      {action && (
        <button type="button" className="ls-link-button" onClick={onAction}>
          {action}
        </button>
      )}
    </div>
  );
}

export function Empty({ title, body }: { title: string; body?: string }) {
  return (
    <div className="ls-empty">
      <p className="ls-muted">{title}</p>
      {body && <p className="ls-mt-1 ls-small ls-faint">{body}</p>}
    </div>
  );
}

export function KeyValue({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <dl className="ls-kv">
      {rows.map(([k, v]) => (
        <div key={k} style={{ display: "contents" }}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}
