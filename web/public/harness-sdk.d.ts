// Types for @localspace/harness-sdk: what a web surface runs against.

export interface Harness {
  readonly harness: string;
  readonly view: string;
  readonly theme: Record<string, string>;
  readonly focused: boolean;
  /** The harness document as Core holds it, as JSON. */
  doc<T = unknown>(): T;
  /** Replace the document. Core reconciles it field by field and commits. */
  write(next: unknown): void;
  /** A message to this harness's logic in Core, at most 64 KB. */
  send(payload: Uint8Array | string | object): void;
  on(event: "doc", fn: (doc: unknown) => void): () => void;
  on(event: "message", fn: (bytes: Uint8Array) => void): () => void;
  on(event: "focus", fn: (focused: boolean) => void): () => void;
  on(event: "command", fn: (command: { name: string; args: unknown }) => void): () => void;
  log(text: string): void;
}

/** Open the bridge. Resolves once the shell has answered with the document. */
export function connect(): Promise<Harness>;

/** Decode a message from the logic as text. */
export function text(bytes: Uint8Array): string;
