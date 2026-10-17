/**
 * 進み具合の行を、失敗した回に残さない（pictkura-dev #68）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { idleLine, withProgressLine, writeProgress } from "../src/progressLine.ts";

/** ステータス行の代わり。値でも関数でも受ける（React の setState と同じ） */
function statusLine(initial = "") {
  let value = initial;
  const set = (u: string | ((prev: string) => string)) => {
    value = typeof u === "function" ? u(value) : u;
  };
  return { get: () => value, set };
}

test("a failed op does not leave its progress on the line", async () => {
  const s = statusLine();
  const line = { current: idleLine() };
  await assert.rejects(
    withProgressLine(line, s.set, async () => {
      writeProgress(line, "Moving… (175 / 300)", s.set);
      throw new Error("one file is held open");
    }),
  );
  assert.equal(s.get(), "");
});

test("a failed op leaves another op's progress alone", async () => {
  const s = statusLine();
  const mine = { current: idleLine() };
  const theirs = { current: idleLine() };
  theirs.current.active = true;
  await assert.rejects(
    withProgressLine(mine, s.set, async () => {
      writeProgress(mine, "Moving… (1 / 1)", s.set);
      writeProgress(theirs, "Exporting… (800 / 3000)", s.set);
      throw new Error("x");
    }),
  );
  assert.equal(s.get(), "Exporting… (800 / 3000)");
});

test("a failure before any progress leaves the line as it was", async () => {
  const s = statusLine("Copied");
  const line = { current: idleLine() };
  await assert.rejects(withProgressLine(line, s.set, async () => Promise.reject(new Error("x"))));
  assert.equal(s.get(), "Copied");
});

test("progress after the op has answered is not written", async () => {
  const s = statusLine();
  const line = { current: idleLine() };
  const result = await withProgressLine(line, s.set, async () => {
    writeProgress(line, "Moving… (25 / 300)", s.set);
    return 300;
  });
  s.set(`Moved ${result}`);
  assert.equal(writeProgress(line, "Moving… (300 / 300)", s.set), false);
  assert.equal(s.get(), "Moved 300");
});

test("success does not touch the line (the caller writes the result)", async () => {
  const s = statusLine();
  const line = { current: idleLine() };
  await withProgressLine(line, s.set, async () => {
    writeProgress(line, "Moving… (25 / 300)", s.set);
  });
  assert.equal(s.get(), "Moving… (25 / 300)");
  assert.equal(line.current.active, false);
});
