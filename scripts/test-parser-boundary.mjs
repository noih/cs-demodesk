import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { test } from 'node:test';
import i18n from 'i18next';
import { translateError } from '../src/errorCodes.ts';

for (const locale of ['en', 'zh-TW', 'zh-CN', 'ja', 'ko', 'ru']) {
  test(`error provenance survives translation (${locale})`, async () => {
    const translation = JSON.parse(await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'));
    await i18n.init({ lng: locale, resources: { [locale]: { translation } } });
    assert.equal(translateError('[demoparser]: events: IllegalPathOp'), `[${translation.errorSources.demoparser}] events: IllegalPathOp`);
    assert.equal(translateError('outer: [FFmpeg]: diagnostic', 'encoding-failed'), `[FFmpeg] ${translation.errors['encoding-failed']}`);
    assert.equal(translateError('ffmpeg mentioned without provenance'), `[${translation.errorSources.unknown}] ffmpeg mentioned without provenance`);
    assert.equal(translateError(undefined), '');
  });
}

test('application code cannot bypass the parser adapter with vendor imports', async () => {
  const root = new URL('../crates/demodesk-core/src/', import.meta.url);
  for (const name of await readdir(root, { recursive: true })) {
    const path = name.replaceAll('\\', '/');
    if (!path.endsWith('.rs') || path === 'parser/demoparser.rs' || path.startsWith('parser/demoparser/')) continue;
    const source = await readFile(new URL(path, root), 'utf8');
    assert.doesNotMatch(source, /(?<!crate::)parser::(?:first_pass|second_pass|parse_demo)/, path);
  }
});
