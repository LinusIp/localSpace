// A model catalog entry from Hugging Face's own facts, for whoever edits
// models/catalog.json: the files of one quantisation with their exact sizes
// and SHA-256 digests (the repository's file listing), and the numbers the
// placement needs (the base model's config.json: layers, KV heads, head size).
// Everything read is treated as data: names, numbers and digests are copied,
// nothing is executed, and a model card's prose is never read at all.
//
//   node scripts/catalog-entry.mjs <gguf repo> <quantisation> <base repo>
//   node scripts/catalog-entry.mjs Qwen/Qwen2.5-7B-Instruct-GGUF q4_k_m Qwen/Qwen2.5-7B-Instruct
//
// Prints the entry's machine-made fields as JSON. The title, the family, the
// licence with its address and the notes are a person's to write, from the
// licence file itself. A gated repository is refused: a tester cannot fetch it.

const [ggufRepo, quant, baseRepo] = process.argv.slice(2);
if (!ggufRepo || !quant || !baseRepo) {
  console.error("usage: node scripts/catalog-entry.mjs <gguf repo> <quantisation> <base repo>");
  process.exit(2);
}
const hub = "https://huggingface.co";

async function json(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} answered ${res.status}`);
  return res.json();
}

const about = await json(`${hub}/api/models/${ggufRepo}`);
if (about.gated) throw new Error(`${ggufRepo} is gated: nobody can download it without an account`);
const tree = await json(`${hub}/api/models/${ggufRepo}/tree/main`);
const wanted = new RegExp(`(^|[-._])${quant.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}([-._]|$)`, "i");
const files = tree
  .filter((f) => f.type === "file" && f.path.endsWith(".gguf") && wanted.test(f.path) && !/mmproj/i.test(f.path))
  .sort((a, b) => a.path.localeCompare(b.path));
if (!files.length) throw new Error(`no ${quant} .gguf in ${ggufRepo}; it has: ${tree.map((f) => f.path).join(", ")}`);
for (const f of files) {
  if (!f.lfs?.oid || !/^[0-9a-f]{64}$/.test(f.lfs.oid)) throw new Error(`${f.path} has no SHA-256 in the listing`);
  if (!Number.isSafeInteger(f.size) || f.size <= 0) throw new Error(`${f.path} has no size in the listing`);
}

const config = await json(`${hub}/${baseRepo}/resolve/main/config.json`);
const text = config.text_config ?? config;
const int = (name) => {
  const value = text[name];
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${baseRepo}/config.json has no usable ${name}`);
  return value;
};
const layers = int("num_hidden_layers");
const heads = int("num_attention_heads");
const kvHeads = text.num_key_value_heads === undefined ? heads : int("num_key_value_heads");
const headSize = text.head_dim === undefined ? int("hidden_size") / heads : int("head_dim");
if (!Number.isInteger(headSize)) throw new Error(`${baseRepo}: the head size is not a whole number`);
const base = await json(`${hub}/api/models/${baseRepo}`);
const parameters = base.safetensors?.total;
const bytes = files.reduce((sum, f) => sum + f.size, 0);

const entry = {
  id: files[0].path.replace(/-0000\d-of-0000\d/, "").replace(/\.gguf$/, "").toLowerCase(),
  params_b: parameters ? Math.round(parameters / 1e7) / 100 : null,
  quant: quant.toUpperCase(),
  bytes,
  context_len: text.max_position_embeddings ?? null,
  repo: ggufRepo,
  files: files.map((f) => f.path),
  verify: Object.fromEntries(files.map((f) => [f.path, { bytes: f.size, sha256: f.lfs.oid }])),
  tensor: {
    core_bytes: bytes,
    routed_expert_bytes: 0,
    layers,
    moe: null,
    // K and V, every layer, every KV head, two bytes a number.
    kv_bytes_per_token_fp16: 2 * layers * kvHeads * headSize * 2,
  },
  licence_tag: (about.tags ?? []).find((t) => t.startsWith("license:")) ?? null,
  at_commit: about.sha,
};
if (text.num_experts || text.num_local_experts) {
  entry.tensor.moe = "a mixture of experts: core_bytes, routed_expert_bytes and moe are a person's to fill";
}
console.log(JSON.stringify(entry, null, 2));
