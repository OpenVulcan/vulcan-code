// Desktop-shell language detection and text loading tests.
// 桌面壳语言检测与文本加载测试。
//
// Catalog coverage and placeholders are checked for every surface by the vulcan-i18n crate tests.
// 目录覆盖与占位符由 vulcan-i18n crate 测试对所有分面统一检查。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  DEFAULT_DESKTOP_LOCALE,
  DESKTOP_LOCALES,
  desktopText,
  detectDesktopLocale,
  formatDesktopText,
  matchDesktopLocaleTag,
  normalizeDesktopLocale,
} from './desktop-text.js';

// MatchingVectors is the shape of the shared behavior vectors.
// MatchingVectors 是共享行为向量的结构。
interface MatchingVectors {
  tags: Array<{ tag: string; locale: string | null }>;
  preferences: Array<{ tags: string[]; locale: string }>;
  savedSelections: Array<{ value: string; locale: string | null }>;
}

// vectors pins the same matching behavior as the WebUI and the Rust implementation.
// vectors 固定与 WebUI 及 Rust 实现相同的匹配行为。
const vectors = JSON.parse(readFileSync(
  new URL('../../../../crates/vulcan-i18n/catalogs/locale-matching.json', import.meta.url),
  'utf8',
)) as MatchingVectors;

test('matches device language tags like every other surface', () => {
  for (const { tag, locale } of vectors.tags) assert.equal(matchDesktopLocaleTag(tag), locale, `tag ${tag}`);
  for (const { tags, locale } of vectors.preferences) assert.equal(detectDesktopLocale(tags), locale, `tags ${tags.join(',')}`);
  for (const { value, locale } of vectors.savedSelections) assert.equal(normalizeDesktopLocale(value), locale, `value ${value}`);
});

// Every registered language loads, and unknown codes fall back to the default language.
// 每种已注册语言都能加载，未知代码回退到默认语言。
test('loads the text of every registered language', () => {
  for (const locale of DESKTOP_LOCALES) assert.ok(desktopText(locale).startupTitle.trim(), `${locale} startup title`);
  assert.equal(desktopText('xx-XX'), desktopText(DEFAULT_DESKTOP_LOCALE));
});

// Placeholders are filled, and unknown ones stay visible.
// 占位符会被填充，未知占位符保持可见。
test('formats placeholders and keeps unknown ones visible', () => {
  assert.equal(formatDesktopText('{count} pages · {missing}', { count: 3 }), '3 pages · {missing}');
});
