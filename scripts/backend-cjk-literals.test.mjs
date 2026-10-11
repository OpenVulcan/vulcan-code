// Exercise the backend CJK literal scanner on synthetic sources covering counted and ignored forms.
// 使用覆盖计数与忽略形式的合成源码验证后端 CJK 字面量扫描器。
import assert from 'node:assert/strict';
import test from 'node:test';
import { countCjkLiterals } from './backend-cjk-literals.mjs';

test('counts CJK literals in production code only', () => {
  const source = [
    'fn fail() -> Result<(), String> {',
    '    // Err(String::from("注释"))',
    '    let english = "plain text";',
    '    Err(format!("失败：{}", r#"原始"#))',
    '}',
    '#[cfg(test)]',
    'mod tests;',
    '#[cfg(test)]',
    'mod inline {',
    '    fn fixture() -> &\'static str { if true { "夹具" } else { "测试" } }',
    '}',
    'const AFTER: &str = "之后";',
  ].join('\n');
  assert.equal(countCjkLiterals(source), 3);
});

test('a test-only function is skipped through its matching brace', () => {
  const source = [
    '#[cfg(test)]',
    'fn helper() { let nested = { "嵌套" }; }',
    'fn real() -> &\'static str { "真实" }',
  ].join('\n');
  assert.equal(countCjkLiterals(source), 1);
});
