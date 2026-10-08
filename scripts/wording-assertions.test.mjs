// Exercise the wording-assertion scanner on synthetic sources covering each counted and ignored form.
// 使用覆盖各计数与忽略形式的合成源码验证文案断言扫描器。
import assert from 'node:assert/strict';
import test from 'node:test';
import { compareWithBaseline, countWordingAssertions, tokenize } from './wording-assertions.mjs';

test('counts Rust assertions whose arguments hold CJK literals', () => {
  const source = [
    'fn check(value: &str) {',
    '    assert_eq!(value, "已完成");',
    '    assert!(',
    '        value.contains("失败"),',
    '        "diagnostic"',
    '    );',
    '    assert_ne!(value, "done");',
    '}',
  ].join('\n');
  assert.equal(countWordingAssertions(source, 'rust'), 2);
});

test('ignores CJK text in Rust comments, raw strings outside assertions and lifetimes', () => {
  const source = [
    "fn pick<'a>(value: &'a str) -> &'a str {",
    '    // assert_eq!(value, "注释中的文案");',
    '    /* assert!(value == "块注释") */',
    '    let fixture = r#"{"text":"夹具"}"#;',
    "    let quote = '\"';",
    '    assert_eq!(value.len(), fixture.len());',
    '    value',
    '}',
  ].join('\n');
  assert.equal(countWordingAssertions(source, 'rust'), 0);
});

test('counts a Rust raw string with CJK only when it sits inside the assertion', () => {
  const source = 'fn t(v: &str) { assert_eq!(v, r#"值 "quoted""#); assert_eq!(v, "ok"); }';
  assert.equal(countWordingAssertions(source, 'rust'), 1);
});

test('counts TypeScript expect chains and text queries with CJK literals', () => {
  const source = [
    'it("渲染标题", () => {',
    '  expect(title).toBe("标题");',
    '  expect(screen.getByRole("button"))',
    '    .toHaveTextContent(`保存 ${count}`);',
    "  expect(node).not.toContain('完成');",
    '  screen.getByText("确认");',
    '  expect(value).toBe("ready");',
    '});',
  ].join('\n');
  assert.equal(countWordingAssertions(source, 'typescript'), 4);
});

test('keeps TypeScript strings that contain comment markers intact', () => {
  const source = "const url = 'http://host/中文'; expect(url).toBe(url); // expect(x).toBe('注释')";
  assert.equal(countWordingAssertions(source, 'typescript'), 0);
  const { code } = tokenize(source, 'typescript');
  assert.equal(code.length, source.length);
});

test('reports files above the baseline and files whose count dropped', () => {
  const counts = new Map([
    ['a.rs', 3],
    ['b.rs', 1],
    ['new.test.ts', 1],
  ]);
  const { exceeded, lowered } = compareWithBaseline(counts, { 'a.rs': 2, 'b.rs': 4, 'gone.rs': 1 });
  assert.deepEqual(exceeded, [
    { file: 'a.rs', count: 3, allowed: 2 },
    { file: 'new.test.ts', count: 1, allowed: 0 },
  ]);
  assert.deepEqual(lowered, [
    { file: 'b.rs', count: 1, allowed: 4 },
    { file: 'gone.rs', count: 0, allowed: 1 },
  ]);
});
