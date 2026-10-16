import test from "node:test";
import assert from "node:assert/strict";
import { getLanguage, resolveLanguage, setLanguage, t, ukrainian } from "./i18n.ts";

test("English is the initial language, independently of device locale", () => {
  assert.equal(getLanguage(), "en");
  assert.equal(resolveLanguage(null), "en");
  assert.equal(resolveLanguage("fr"), "en");
  assert.equal(t("Print"), "Print");
});

test("saved Ukrainian preference is honored and messages interpolate", () => {
  assert.equal(resolveLanguage("uk"), "uk");
  setLanguage("uk");
  assert.equal(t("Print"), "Надрукувати");
  assert.equal(t("Printed {copies} copies.", { copies: 3 }), "Надруковано копій: 3.");
  setLanguage("en");
  assert.equal(t("Printed {copies} copies.", { copies: 3 }), "Printed 3 copies.");
});

test("all translations preserve the English interpolation fields", () => {
  const fields = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort();
  for (const [english, translation] of Object.entries(ukrainian)) {
    assert.deepEqual(fields(translation), fields(english), english);
    assert.ok(translation.trim());
  }
});
