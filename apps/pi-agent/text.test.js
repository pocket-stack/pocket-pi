import { expect, test } from "bun:test";

globalThis.View = { measureText: (value) => [...value].length * 8 };
await import("./text.js");
const { wrapTextPage } = globalThis.PiText;

test("wraps words and unbroken tokens within maxWidth", () => {
  const page = wrapTextPage("alpha beta 123456789", {}, 48, 0, 0, 10);
  const lines = page.text.split("\n");
  expect(lines).toEqual(["alpha", "beta", "123456", "789"]);
  expect(lines.every((line) => [...line].length * 8 <= 48)).toBe(true);
  expect(page.hasMore).toBe(false);
});

test("paginates deterministically to EOF and always advances", () => {
  const source = "one two three four five six seven eight";
  const rendered = [];
  let offset = 0;
  let sourceLine = 0;
  let finished = false;
  for (let iteration = 0; iteration <= source.length; iteration += 1) {
    const page = wrapTextPage(source, {}, 40, offset, sourceLine, 2);
    expect(wrapTextPage(source, {}, 40, offset, sourceLine, 2)).toEqual(page);
    expect(page.text.split("\n").length).toBeLessThanOrEqual(2);
    rendered.push(page.text);
    if (!page.hasMore) {
      finished = true;
      break;
    }
    expect(page.nextOffset).toBeGreaterThan(offset);
    offset = page.nextOffset;
    sourceLine = page.nextSourceLine;
  }
  expect(finished).toBe(true);
  expect(rendered.join("\n").split(/\s+/)).toEqual(source.split(/\s+/));
});

test("tracks source lines across LF and CRLF", () => {
  const source = "aa\nbb\r\ncc";
  const first = wrapTextPage(source, {}, 80, 0, 0, 2);
  const second = wrapTextPage(source, {}, 80, first.nextOffset, first.nextSourceLine, 2);
  expect(first).toEqual({
    text: "aa\nbb", nextOffset: "aa\nbb\r\n".length, startSourceLine: 0,
    nextSourceLine: 2, lastSourceLine: 1, hasMore: true,
  });
  expect(second).toEqual({
    text: "cc", nextOffset: source.length, startSourceLine: 2,
    nextSourceLine: 2, lastSourceLine: 2, hasMore: false,
  });
});
