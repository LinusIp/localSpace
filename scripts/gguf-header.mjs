// What a GGUF file says of itself, read from its header alone: the
// architecture, the layer count, the attention heads and the key and value
// lengths, the expert counts of a mixture of experts, the context length,
// and the bytes of its tensors, split into the routed experts' and the rest.
// It is how a catalog entry's planner sizes are taken from the real file
// (docs/DECISIONS.md, 2026-10-07, document 29 §3), for a file on this disk
// or one on Hugging Face, of which only the header is fetched.
//
//   node scripts/gguf-header.mjs <path or https URL> [--json] [--all] [--tensors]
//
// The header is read in steps of 8 MB until it parses whole; a model with a
// large vocabulary carries a few MB of tokens before its tensor list.

import { openSync, readSync, closeSync, statSync } from "node:fs";

const [target, ...flags] = process.argv.slice(2);
if (!target) {
  console.error("usage: node scripts/gguf-header.mjs <path or https URL> [--json]");
  process.exit(2);
}
const asJson = flags.includes("--json");
const all = flags.includes("--all");
const listTensors = flags.includes("--tensors");
const STEP = 8 * 1024 * 1024;

/** The first `n` bytes of the file or URL. */
async function head(n) {
  if (/^https?:\/\//.test(target)) {
    const res = await fetch(target, { headers: { Range: `bytes=0-${n - 1}` }, redirect: "follow" });
    if (!res.ok && res.status !== 206) throw new Error(`${res.status} fetching ${target}`);
    return Buffer.from(await res.arrayBuffer());
  }
  const size = statSync(target).size;
  const fd = openSync(target, "r");
  const buf = Buffer.alloc(Math.min(n, size));
  readSync(fd, buf, 0, buf.length, 0);
  closeSync(fd);
  return buf;
}

class Short extends Error {}

/** Parses the header out of `buf`; throws `Short` when more bytes are needed. */
function parse(buf) {
  let at = 0;
  const need = (n) => {
    if (at + n > buf.length) throw new Short();
  };
  const u32 = () => {
    need(4);
    const v = buf.readUInt32LE(at);
    at += 4;
    return v;
  };
  const u64 = () => {
    need(8);
    const v = buf.readBigUInt64LE(at);
    at += 8;
    return v;
  };
  const str = () => {
    const n = Number(u64());
    need(n);
    const s = buf.toString("utf8", at, at + n);
    at += n;
    return s;
  };
  const scalar = (type) => {
    switch (type) {
      case 0: need(1); return buf.readUInt8(at++);
      case 1: need(1); return buf.readInt8(at++);
      case 2: need(2); { const v = buf.readUInt16LE(at); at += 2; return v; }
      case 3: need(2); { const v = buf.readInt16LE(at); at += 2; return v; }
      case 4: return u32();
      case 5: need(4); { const v = buf.readInt32LE(at); at += 4; return v; }
      case 6: need(4); { const v = buf.readFloatLE(at); at += 4; return v; }
      case 7: need(1); return buf.readUInt8(at++) !== 0;
      case 8: return str();
      case 10: return Number(u64());
      case 11: need(8); { const v = Number(buf.readBigInt64LE(at)); at += 8; return v; }
      case 12: need(8); { const v = buf.readDoubleLE(at); at += 8; return v; }
      default: throw new Error(`unknown GGUF value type ${type}`);
    }
  };
  const value = (type) => {
    if (type !== 9) return scalar(type);
    const inner = u32();
    const count = Number(u64());
    const items = [];
    for (let i = 0; i < count; i++) items.push(inner === 9 ? value(9) : scalar(inner));
    return items;
  };
  if (buf.toString("ascii", 0, 4) !== "GGUF") throw new Error("not a GGUF file");
  at = 4;
  const version = u32();
  const tensors = Number(u64());
  const kvs = Number(u64());
  const meta = {};
  for (let i = 0; i < kvs; i++) {
    const key = str();
    const type = u32();
    const v = value(type);
    // Vocabularies and merges are long and not wanted here.
    meta[key] = Array.isArray(v) && v.length > 64 ? `[${v.length} items]` : v;
  }
  // The tensor list: name, dimensions, type, offset. Bytes per element by type.
  const bytesPer = {
    0: [4, 1], 1: [2, 1], 2: [18, 32], 3: [20, 32], 6: [22, 32], 7: [24, 32], 8: [34, 32], 9: [36, 32],
    10: [84, 256], 11: [110, 256], 12: [144, 256], 13: [176, 256], 14: [210, 256], 15: [292, 256],
    16: [66, 256], 17: [74, 256], 18: [56, 256], 19: [50, 256], 20: [82, 256], 21: [58, 256],
    22: [8, 1], 23: [56, 256], 24: [1, 1], 25: [2, 1], 26: [4, 1], 27: [8, 1], 28: [1, 1], 29: [4, 1],
    30: [2, 1], 31: [1, 1], 32: [1, 1], 34: [66, 256], 36: [1, 1], 37: [4, 1], 38: [17, 32], 39: [2, 1],
  };
  let total = 0n;
  let experts = 0n;
  // Parameters as well as bytes: a mixture of experts' active count is the
  // whole less the routed experts a token does not visit.
  let paramsTotal = 0n;
  let paramsExperts = 0n;
  let unknownType = null;
  const named = [];
  for (let i = 0; i < tensors; i++) {
    const name = str();
    const dims = u32();
    let elements = 1n;
    for (let d = 0; d < dims; d++) elements *= u64();
    const type = u32();
    u64(); // the offset
    const size = bytesPer[type];
    if (!size) {
      unknownType = type;
      continue;
    }
    const bytes = (elements * BigInt(size[0])) / BigInt(size[1]);
    total += bytes;
    paramsTotal += elements;
    if (/_exps\.|\.experts?\.|ffn_(gate|up|down)_exps/.test(name)) paramsExperts += elements;
    if (/^blk.0./.test(name) || !/^blk./.test(name)) named.push(`${name} ${type} ${bytes}`);
    if (/_exps\.|\.experts?\.|ffn_(gate|up|down)_exps/.test(name)) experts += bytes;
  }
  return { version, tensors, meta, total, experts, paramsTotal, paramsExperts, unknownType, named };
}

let n = STEP;
let out;
for (;;) {
  const buf = await head(n);
  try {
    out = parse(buf);
    break;
  } catch (e) {
    if (!(e instanceof Short)) throw e;
    if (buf.length < n) throw new Error("the file ended inside its header");
    n += STEP;
    if (n > 512 * 1024 * 1024) throw new Error("no whole header within 512 MB");
  }
}
const { meta } = out;
const arch = meta["general.architecture"];
const g = (k) => meta[`${arch}.${k}`];
const summary = {
  architecture: arch,
  name: meta["general.name"],
  size_label: meta["general.size_label"],
  file_type: meta["general.file_type"],
  context_length: g("context_length"),
  block_count: g("block_count"),
  attention_head_count: g("attention.head_count"),
  attention_head_count_kv: g("attention.head_count_kv"),
  key_length: g("attention.key_length"),
  value_length: g("attention.value_length"),
  embedding_length: g("embedding_length"),
  full_attention_interval: g("full_attention_interval"),
  expert_count: g("expert_count"),
  expert_used_count: g("expert_used_count"),
  expert_shared_count: g("expert_shared_count"),
  tensor_bytes_total: Number(out.total),
  tensor_bytes_experts: Number(out.experts),
  tensor_bytes_core: Number(out.total - out.experts),
  params_total: Number(out.paramsTotal),
  params_experts: Number(out.paramsExperts),
  // What one token visits: everything but the routed experts it is not
  // sent to (docs/DECISIONS.md, 2026-10-08: the active count comes from the
  // file, not from the name).
  params_active:
    g("expert_count") && g("expert_used_count")
      ? Number(out.paramsTotal - out.paramsExperts) + Math.round((Number(out.paramsExperts) * g("expert_used_count")) / g("expert_count"))
      : Number(out.paramsTotal),
  tensors: out.tensors,
  unknown_tensor_type: out.unknownType,
};
if (listTensors) for (const line of out.named) console.log(line);
if (all) for (const [k, v] of Object.entries(meta)) console.log(`${k}: ${JSON.stringify(v)}`);
if (asJson) {
  console.log(JSON.stringify(summary, null, 1));
} else {
  for (const [k, v] of Object.entries(summary)) if (v !== undefined && v !== null) console.log(`${k}: ${typeof v === "number" && v > 1e6 ? v.toLocaleString("en-US") : v}`);
}
