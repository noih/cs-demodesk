import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createInstance } from 'i18next';

const languages = ['en', 'zh-TW', 'zh-CN', 'ja', 'ko', 'ru'];
const locales = Object.fromEntries(await Promise.all(languages.map(async language => [language,
  JSON.parse(await readFile(new URL(`../src/i18n/locales/${language}.json`, import.meta.url), 'utf8')),
])));
const flatten = (value, prefix = '') => Object.fromEntries(Object.entries(value).flatMap(([key, entry]) =>
  typeof entry === 'string' ? [[prefix + key, entry]] : Object.entries(flatten(entry, `${prefix}${key}.`))));
const base = flatten(locales.en);
const singularKey = key => key.replace(/_(zero|one|two|few|many|other)$/, '');
const placeholders = value => [...value.matchAll(/{{\s*([^}]+?)\s*}}|<([\w]+)\s*\/>/g)].map(match => match[0]).sort();
for (const language of languages) {
  const entries = flatten(locales[language]);
  assert.deepEqual([...new Set(Object.keys(entries).map(singularKey))].sort(), [...new Set(Object.keys(base).map(singularKey))].sort(), `${language}: keys`);
  for (const [key, value] of Object.entries(entries)) {
    const reference = base[key] ?? base[`${singularKey(key)}_other`];
    assert.ok(value.trim(), `${language}: empty ${key}`);
    assert.deepEqual(placeholders(value), placeholders(reference), `${language}: parameters in ${key}`);
  }
  const i18n = createInstance();
  await i18n.init({ lng: language, resources: { [language]: { translation: locales[language] } }, fallbackLng: false });
  assert.equal(i18n.t('highlights.tags.wallbang'), locales[language].highlights.tags.wallbang);
  assert.equal(i18n.t('highlights.tags.3k', { defaultValue: '3k' }), '3k');
  assert.equal(i18n.t('highlights.tags.future-tag', { defaultValue: 'future-tag' }), 'future-tag');
  console.log(`${language}: locale keys, parameters and highlight tags passed`);
}
