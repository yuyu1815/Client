import assert from "node:assert/strict";
import test from "node:test";
import { createLanguageRequests } from "../src/lib/languageRequests.js";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => (resolve = done));
  return { promise, resolve };
};

test("serializes deferred language writes in request order", async () => {
  const calls = [];
  const first = deferred();
  const second = deferred();
  const requests = createLanguageRequests((language) => {
    calls.push(language);
    return (language === "ja" ? first : second).promise;
  });

  const loadGeneration = requests.generation;
  const ja = requests.set("ja");
  const en = requests.set("en");
  assert.equal(requests.isCurrent(loadGeneration), false);
  assert.equal(requests.generation, 2);
  await Promise.resolve();
  assert.deepEqual(calls, ["ja"]);

  first.resolve({ ok: true });
  await ja;
  await Promise.resolve();
  assert.deepEqual(calls, ["ja", "en"]);
  second.resolve({ ok: true });
  await en;
  assert.deepEqual(calls, ["ja", "en"]);
});
