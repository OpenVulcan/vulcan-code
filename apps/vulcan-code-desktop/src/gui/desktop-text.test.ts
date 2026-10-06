// Desktop-shell text coverage and language detection tests.
// 桌面壳文本覆盖与语言检测测试。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { DESKTOP_TEXT, detectDesktopLocale, formatDesktopText, type DesktopLocale } from './desktop-text.js';

// placeholders lists the `{name}` placeholders of one template in a stable order.
// placeholders 以稳定顺序列出一个模板中的 `{name}` 占位符。
function placeholders(template: string): string {
  return [...template.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort().join(',');
}

// Every language defines every text with exactly the English placeholders.
// 每种语言都定义全部文本，且占位符与英文完全一致。
test('every desktop language covers every text with matching placeholders', () => {
  const english = DESKTOP_TEXT['en-US'];
  for (const [locale, text] of Object.entries(DESKTOP_TEXT)) {
    for (const [key, template] of Object.entries(english)) {
      const translated = text[key as keyof typeof english];
      assert.ok(translated.trim(), `${locale}.${key} must not be empty`);
      assert.equal(placeholders(translated), placeholders(template), `${locale}.${key} placeholders`);
    }
  }
});

// The desktop list mirrors the shared registry that owns the supported languages.
// 桌面列表与持有受支持语言的共享注册表保持一致。
test('desktop languages match the shared language registry', () => {
  const registry = JSON.parse(readFileSync(new URL('../../../../crates/vulcan-i18n/catalogs/locales.json', import.meta.url), 'utf8')) as { locales: Array<{ code: string }> };
  const locales = registry.locales.map((entry) => entry.code).sort();
  assert.deepEqual(Object.keys(DESKTOP_TEXT).sort(), locales, 'update DESKTOP_TEXT when the shared language list changes');
});

// Device tags resolve by language, Chinese by script and region, and unsupported languages fall back to English.
// 设备标签按语言解析，中文按书写系统与地区解析，不受支持的语言回退到英文。
test('detects the device language with Chinese script handling and an English fallback', () => {
  // Cases cover separate Hong Kong and Taiwan catalogs, with explicit script taking precedence over region.
  // 用例覆盖独立的香港与台湾目录，显式书写系统优先于地区。
  const cases: Array<[string[], DesktopLocale]> = [
    [['zh-Hant'], 'zh-TW'],
    [['zh-Hant-TW'], 'zh-TW'],
    [['zh-Hant-HK'], 'zh-HK'],
    [['zh-TW'], 'zh-TW'],
    [['zh-HK'], 'zh-HK'],
    [['zh_HK'], 'zh-HK'],
    [['zh-MO'], 'zh-TW'],
    [['zh-Hans-HK'], 'zh-CN'],
    [['zh-Hans-SG'], 'zh-CN'],
    [['zh'], 'zh-CN'],
    [['pt-PT'], 'pt-BR'],
    [['es-MX'], 'es-ES'],
    [['en-GB'], 'en-US'],
    [['ar-SA', 'fr-CA'], 'fr-FR'],
    [['ar-SA'], 'en-US'],
    [[], 'en-US'],
  ];
  for (const [tags, expected] of cases) {
    assert.equal(detectDesktopLocale(tags), expected, tags.join(','));
  }
});

// Placeholders are filled, and unknown ones stay visible.
// 占位符会被填充，未知占位符保持可见。
test('formats placeholders and keeps unknown ones visible', () => {
  assert.equal(formatDesktopText('{count} pages · {missing}', { count: 3 }), '3 pages · {missing}');
});
