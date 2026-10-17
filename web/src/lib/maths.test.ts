import { test } from "node:test";
import assert from "node:assert/strict";
import { CLOSE, OPEN, extractMaths, splitAtMaths } from "./maths.ts";

const at = (n: number) => `${OPEN}${n}${CLOSE}`;

test("a display formula and an inline one are lifted out, in order", () => {
  const { text, parts } = extractMaths("The area is\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\nand $a_1 + b_1$ too.");
  assert.equal(text, `The area is\n${at(0)}\nand ${at(1)} too.`);
  assert.deepEqual(parts, [
    { tex: "\\int_0^1 x^2\\,dx = \\frac{1}{3}", display: true },
    { tex: "a_1 + b_1", display: false },
  ]);
});

test("brackets and parentheses count as models write them", () => {
  const { parts } = extractMaths("So \\(x^2\\) grows, and\n\\[\n\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}\n\\]");
  assert.deepEqual(parts, [
    { tex: "x^2", display: false },
    { tex: "\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}", display: true },
  ]);
});

test("prices stay prices", () => {
  for (const words of ["It costs $5 and $10.", "Between $5, or $6 at most.", "US$ 20 and $ 30", "an escaped \\$x$ stays"]) {
    assert.deepEqual(extractMaths(words).parts, [], words);
    assert.equal(extractMaths(words).text, words);
  }
});

test("code, fenced or inline, is left exactly as it is", () => {
  const answer = "Use `$PATH` here:\n```bash\necho $HOME and $$x$$\n```\nthen $y$.";
  const { text, parts } = extractMaths(answer);
  assert.equal(text, `Use \`$PATH\` here:\n\`\`\`bash\necho $HOME and $$x$$\n\`\`\`\nthen ${at(0)}.`);
  assert.deepEqual(parts, [{ tex: "y", display: false }]);
});

test("the placeholder's marks cannot be smuggled in by the answer", () => {
  const { text, parts } = extractMaths(`a ${OPEN}0${CLOSE} b`);
  assert.equal(text, "a 0 b");
  assert.deepEqual(parts, []);
});

test("a text splits into its plain pieces and its formulas", () => {
  assert.deepEqual(splitAtMaths(`x ${at(0)} y ${at(1)}`), ["x ", 0, " y ", 1]);
  assert.deepEqual(splitAtMaths("plain"), ["plain"]);
});
