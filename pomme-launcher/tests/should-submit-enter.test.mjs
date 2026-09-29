import assert from "node:assert/strict";
import test from "node:test";
import { shouldSubmitEnter } from "../src/lib/shouldSubmitEnter.mjs";

const event = (key, isComposing = false, keyCode = 13) => ({
  key,
  nativeEvent: { isComposing, keyCode },
});

test("submits on Enter but ignores IME conversion Enter events", () => {
  assert.equal(shouldSubmitEnter(event("Enter")), true);
  assert.equal(shouldSubmitEnter(event("Enter", true)), false);
  assert.equal(shouldSubmitEnter(event("Enter", false, 229)), false);
  assert.equal(shouldSubmitEnter(event("a")), false);
});
