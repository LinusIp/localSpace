# 2026-10-31: LFM3 in the preview

Liquid AI's LFM3-1.7B and LFM3-VL-2B join the catalog behind a switch. They need the engine pinned in [2026-10-31-engine-b11960.md](2026-10-31-engine-b11960.md).

- **The switch:** `lfm3` (`Feature::Lfm3`), on only in `features.toml`'s preview channel.
- **While it is off:** the app's Core leaves the entries out as it starts, as it does a hidden entry. An entry names its switch with `"switch": "lfm3"`.
- **Turning it:** takes effect when the app next starts.
- **`localspace measure`:** keeps every entry, as it keeps hidden ones.
- **The licence (both repositories' LICENSE files):** LFM Open License v1.0. Its §5 licenses commercial use only below an annual revenue of $10,000,000.
  - The catalog's words: "LFM Open License: free for personal use and for organisations under $10M yearly revenue; larger organisations need a licence from Liquid AI."
  - `commercial_use` is `false`, since not every commercial use is licensed. The default recommendation offers only `commercial_use` entries, so LFM3 is never anyone's default, and never the smaller picture model Core suggests when the current one can't keep its picture file (`picture_model`).
- **Files:**
  - Liquid AI's 4-bit-trained QAD-Q4_0 weights (1,049,615,680 and 1,049,615,648 bytes);
  - for LFM3-VL, the F16 vision file (378,996,032 bytes).
  - All are pinned to the repositories' revisions with their SHA-256.
- **Memory:** 11 of the 24 layers have attention (`config.json`'s `layer_types`), so the KV figure is 22,528 bytes per token at FP16, not the 49,152 the 24 layers would give.
- **Thinking:**
  - the template thinks unless told not to, so thinking is off by default, with a budget of 1,024 tokens;
  - sampling as the model cards give it: temperature 0.1, top-k 50, repeat penalty 1.1, and for thinking temperature 1.0, top-p 0.95, top-k 20.
- **Tools:** offered. The template writes tool calls as `<tool_call><function=…>`. The engine test ([2026-10-31-engine-test.md](2026-10-31-engine-test.md)) shows whether the engine reads them; if it doesn't, `supports_tools` turns off before the preview.
- **Test:** `an_entry_behind_a_switch_is_left_out_while_it_is_off` (`models.rs`); `only_the_preview_turns_a_feature_on` (`features.rs`).
