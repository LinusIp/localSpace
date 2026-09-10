// Types for @localspace/harness-sdk: what a web surface runs against.

export interface Harness {
  readonly harness: string;
  readonly view: string;
  readonly theme: Record<string, string>;
  readonly focused: boolean;
  /** The number of the last write sent. */
  readonly lastWrite: number;
  /** The harness document as Core holds it, as JSON. */
  doc<T = unknown>(): T;
  /**
   * Replace the document. Core reconciles it field by field and commits the
   * difference as the user's edit. `{ commit: false }` moves the document
   * without a commit: for state that is the user's but not an edit, such as
   * the selection, so undo steps over it. Returns the write's number;
   * documents that arrive with `written` below it were read before this
   * write landed.
   */
  write(next: unknown, options?: { commit?: boolean }): number;
  /** A message to this harness's logic in Core, at most 64 KB. */
  send(payload: Uint8Array | string | object): void;
  /** Undo and redo are the environment's, through the history, not the surface's. */
  undo(): void;
  redo(): void;
  /** Tell the shell what the surface shows, so its controls stay true. */
  report(status: { zoom?: number }): void;
  /** `written` is the number of the surface's last write Core had taken in when this document was read. */
  on(event: "doc", fn: (doc: unknown, meta: { written: number }) => void): () => void;
  on(event: "message", fn: (bytes: Uint8Array) => void): () => void;
  on(event: "focus", fn: (focused: boolean) => void): () => void;
  on(event: "command", fn: (command: { name: string; args: unknown }) => void): () => void;
  log(text: string): void;
}

/** Open the bridge. Resolves once the shell has answered with the document. */
export function connect(): Promise<Harness>;

/** Decode a message from the logic as text. */
export function text(bytes: Uint8Array): string;
