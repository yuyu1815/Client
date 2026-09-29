import assert from "node:assert/strict";
import test from "node:test";
import { createLanguageRequests } from "../src/lib/languageRequests.js";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => (resolve = done));
  return { promise, resolve };
};

const setup = (load, update) => {
  const requests = createLanguageRequests(update);
  let settings = { language: "en", keepLauncherOpen: true, launchWithConsole: false };
  let languageChanged = false;
  const loading = load.then((loaded) => {
    settings = {
      language: languageChanged ? settings.language : loaded.language,
      keepLauncherOpen: loaded.keepLauncherOpen,
      launchWithConsole: loaded.launchWithConsole,
    };
  });
  const setLanguage = (language) =>
    requests.set(language).then((result) => {
      if (result.ok) {
        languageChanged = true;
        settings = { ...settings, language };
      }
    });
  return {
    get settings() {
      return settings;
    },
    loading,
    setLanguage,
  };
};

test("failed language write does not hide the language from delayed settings load", async () => {
  const load = deferred();
  const write = deferred();
  const state = setup(load.promise, () => write.promise);
  const setting = state.setLanguage("ja");

  load.resolve({ language: "ja", keepLauncherOpen: false, launchWithConsole: true });
  await state.loading;
  write.resolve({ ok: false, error: "write failed" });
  await setting;

  assert.deepEqual(state.settings, {
    language: "ja",
    keepLauncherOpen: false,
    launchWithConsole: true,
  });
});

test("successful language write before load keeps it while loading other settings", async () => {
  const load = deferred();
  const write = deferred();
  const state = setup(load.promise, () => write.promise);
  const setting = state.setLanguage("fr");

  write.resolve({ ok: true });
  await setting;
  load.resolve({ language: "ja", keepLauncherOpen: false, launchWithConsole: true });
  await state.loading;

  assert.deepEqual(state.settings, {
    language: "fr",
    keepLauncherOpen: false,
    launchWithConsole: true,
  });
});

test("successful language write after load wins", async () => {
  const load = deferred();
  const write = deferred();
  const state = setup(load.promise, () => write.promise);
  const setting = state.setLanguage("fr");

  load.resolve({ language: "ja", keepLauncherOpen: false, launchWithConsole: true });
  await state.loading;
  assert.equal(state.settings.language, "ja");
  write.resolve({ ok: true });
  await setting;

  assert.equal(state.settings.language, "fr");
});

test("overlapping successful writes keep the latest persisted language", async () => {
  const load = deferred();
  const firstWrite = deferred();
  const secondWrite = deferred();
  const calls = [];
  const state = setup(load.promise, (language) => {
    calls.push(language);
    return language === "fr" ? firstWrite.promise : secondWrite.promise;
  });
  const first = state.setLanguage("fr");
  const second = state.setLanguage("ja");
  load.resolve({ language: "en", keepLauncherOpen: false, launchWithConsole: true });
  await state.loading;

  firstWrite.resolve({ ok: true });
  await first;
  assert.deepEqual(calls, ["fr", "ja"]);
  secondWrite.resolve({ ok: true });
  await second;

  assert.equal(state.settings.language, "ja");
  assert.equal(state.settings.keepLauncherOpen, false);
  assert.equal(state.settings.launchWithConsole, true);
});
