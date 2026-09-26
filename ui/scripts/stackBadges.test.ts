/**
 * 重ねの印の形を幅で選ぶ（`src/stackBadges.ts`、dev #32・#167）。`npm --prefix ui test`。
 *
 * 幅は1文字 6px の固定で測る（`measure`）。印1つの幅は「文字＋左右の余白」:
 * `RAW+JPEG` 48+10=58、`▤ Burst 12` 60+10=70、`▤ 12` 24+10=34（詰めて 30）、
 * 記号 13+10=23（詰めて 19）、`▤` 6+6=12（詰めた形だけ）。間は 3px（詰めて 2px）
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { BURST_GLYPH, chooseBadges, type BadgeChoice } from "../src/stackBadges.ts";

const measure = (s: string) => [...s].length * 6;
const burst = (n: string) => ({ long: `▤ Burst ${n}`, short: `▤ ${n}` });
/** 形を短い文字にする（`text`＝RAW+JPEG、`icon`＝記号、他は連写の文字）。詰めた形は末尾に `/t` */
const shape = (c: BadgeChoice) =>
  c.parts.map((p) => (p.kind === "pair-text" ? "text" : p.kind === "pair-icon" ? "icon" : p.text)).join(" + ") +
  (c.tight ? " /t" : "");

test("組だけ: 入れば文字、入らなければ記号、それも入らなければ詰めた記号", () => {
  const pick = (room: number) => shape(chooseBadges(room, { pair: true }, measure));
  assert.equal(pick(100), "text");
  assert.equal(pick(58), "text");
  assert.equal(pick(57), "icon");
  assert.equal(pick(23), "icon");
  assert.equal(pick(22), "icon /t");
  // どれも入らない: 最後の候補（詰めた記号）
  assert.equal(pick(5), "icon /t");
});

test("連写だけ: 言葉つき → 短い形 → 詰めた短い形 → 数を出さない ▤", () => {
  const pick = (room: number) => shape(chooseBadges(room, { pair: false, burst: burst("12") }, measure));
  assert.equal(pick(70), "▤ Burst 12");
  assert.equal(pick(69), "▤ 12");
  assert.equal(pick(34), "▤ 12");
  assert.equal(pick(33), "▤ 12 /t");
  assert.equal(pick(30), "▤ 12 /t");
  assert.equal(pick(29), `${BURST_GLYPH} /t`);
  assert.equal(pick(3), `${BURST_GLYPH} /t`);
});

test("組かつ連写: 文字＋数 → 記号＋数 → 詰めた記号＋数 → 数だけ → ▤", () => {
  const pick = (room: number) => shape(chooseBadges(room, { pair: true, burst: burst("12") }, measure));
  // 連写の言葉つきの形は、組と並ぶときは使わない（短い形だけ）
  assert.equal(pick(200), "text + ▤ 12");
  assert.equal(pick(95), "text + ▤ 12");
  assert.equal(pick(94), "icon + ▤ 12");
  assert.equal(pick(60), "icon + ▤ 12");
  assert.equal(pick(59), "icon + ▤ 12 /t");
  assert.equal(pick(51), "icon + ▤ 12 /t");
  // 組の記号を外してでもコマ数を残す
  assert.equal(pick(50), "▤ 12 /t");
  assert.equal(pick(30), "▤ 12 /t");
  assert.equal(pick(29), `${BURST_GLYPH} /t`);
});

test("連写の数を切って見せない: 出る文字は言葉つき・短い形・▤ のどれか（`▤ 1,234` が `234` にならない）", () => {
  for (const n of ["2", "12", "150", "1,234", "12,345"]) {
    const b = burst(n);
    for (const pair of [false, true]) {
      for (let room = 0; room <= 220; room++) {
        const c = chooseBadges(room, { pair, burst: b }, measure);
        for (const p of c.parts) {
          if (p.kind !== "burst") continue;
          assert.ok(
            [b.long, b.short, BURST_GLYPH].includes(p.text),
            `n=${n} pair=${pair} room=${room}: ${p.text}`,
          );
        }
      }
    }
  }
});

test("入るものがあれば、選んだ形は幅に収まる（入らないのは最後の候補だけ）", () => {
  const widthOf = (c: BadgeChoice) => {
    const pad = c.tight ? 3 : 5;
    const gap = c.tight ? 2 : 3;
    return (
      c.parts
        .map((p) => (p.kind === "pair-text" ? measure("RAW+JPEG") : p.kind === "pair-icon" ? 13 : measure(p.text)))
        .reduce((s, w) => s + w + 2 * pad, 0) +
      gap * (c.parts.length - 1)
    );
  };
  for (const labels of [
    { pair: true },
    { pair: false, burst: burst("150") },
    { pair: true, burst: burst("1,234") },
  ]) {
    // 最後の候補の幅（どれも入らないときに出る形）
    const last = chooseBadges(0, labels, measure);
    for (let room = 0; room <= 220; room++) {
      const c = chooseBadges(room, labels, measure);
      if (shape(c) === shape(last) && widthOf(c) > room) continue;
      assert.ok(widthOf(c) <= room, `${JSON.stringify(labels)} room=${room}: ${shape(c)} = ${widthOf(c)}px`);
    }
  }
});

test("重ねでなければ印は無い", () => {
  assert.deepEqual(chooseBadges(100, { pair: false }, measure), { parts: [], tight: false });
});

test("余白と間の値は App.css と同じ（幅の見積もりがずれると、入らない形を選ぶ）", () => {
  const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");
  const block = (sel: string) => {
    const i = css.indexOf(`${sel} {`);
    assert.ok(i >= 0, sel);
    return css.slice(i, css.indexOf("}", i));
  };
  assert.match(block(".cell-chips"), /\n {2}gap: 3px;/);
  assert.match(block(".cell-chip"), /\n {2}padding: 0 5px;/);
  assert.match(block(".cell-chips.tight"), /\n {2}gap: 2px;/);
  assert.match(block(".cell-chips.tight .cell-chip"), /\n {2}padding: 0 3px;/);
});
