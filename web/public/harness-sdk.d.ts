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
  /** Core's Automerge document, saved, when connected with `sync: true`; else null. */
  snapshot(): Uint8Array | null;
  /**
   * Replace the document. Core reconciles it field by field and commits the
   * difference as the user's edit. `{ commit: false }` moves the document
   * without a commit: for state that is the user's but not an edit, such as
   * the selection, so undo steps over it. Returns the write's number;
   * documents that arrive with `written` below it were read before this
   * write landed.
   */
  write(next: unknown, options?: { commit?: boolean }): number;
  /** An Automerge sync message for Core, from the surface's replica. */
  sync(message: Uint8Array): void;
  /** A message to this harness's logic in Core, at most 64 KB. */
  send(payload: Uint8Array | string | object): void;
  /** Undo and redo are the environment's, through the history, not the surface's. */
  undo(): void;
  redo(): void;
  /** Tell the shell what the surface shows, so its controls stay true. */
  report(status: { zoom?: number }): void;
  /**
   * A file rendered from this harness's document — a PNG or an SVG of a
   * board — for Core to keep as a document of its own and register as a
   * typed artifact of `kind`. `name` is the stem; Core adds the commit it
   * shows and the kind's extension. `bytes` is transferred, not copied.
   * The shell answers with an "artifact" event.
   */
  export(artifact: {
    kind: string;
    name: string;
    mime: string;
    bytes: ArrayBuffer | Uint8Array;
    fields?: Record<string, unknown>;
    summary?: string;
  }): void;
  /** What became of an `export`: the artifact's id, the file's name and size, or the error. */
  on(event: "artifact", fn: (result: { ok: boolean; id?: string; name?: string; bytes?: number; error?: string }) => void): () => void;
  /** `written` is the number of the surface's last write Core had taken in when this document was read. */
  on(event: "doc", fn: (doc: unknown, meta: { written: number }) => void): () => void;
  /** An Automerge sync message from Core, for the surface's replica. */
  on(event: "sync", fn: (message: Uint8Array) => void): () => void;
  on(event: "message", fn: (bytes: Uint8Array) => void): () => void;
  on(event: "focus", fn: (focused: boolean) => void): () => void;
  on(event: "command", fn: (command: { name: string; args: unknown }) => void): () => void;
  log(text: string): void;
}

export interface ConnectOptions {
  /** Hold the document as an Automerge replica and exchange sync messages, rather than JSON. */
  sync?: boolean;
}

/** Open the bridge. Resolves once the shell has answered with the document. */
export function connect(options?: ConnectOptions): Promise<Harness>;

/** Decode a message from the logic as text. */
export function text(bytes: Uint8Array): string;
