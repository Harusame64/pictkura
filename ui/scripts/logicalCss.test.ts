/**
 * 右から左へ書く言語（アラビア語・ヘブライ語）に備えて、画面の左右は**論理プロパティ**で書く。
 * `margin-left` ではなく `margin-inline-start`、`right` ではなく `inset-inline-end`。
 * 横書きの左→右では物理と同じ値になるので、いまの見た目は変わらない。`dir="rtl"` で鏡に映る。
 *
 * 物理のまま残すものは下の `KEEP` に**理由ごと**書く。新しく左右を書いたらここで落ちる。
 * `npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = new URL("../src/", import.meta.url);
const SHEETS = ["App.css", "tokens.css"];

/** 物理のまま残すもの。鍵は「規則の頭 + 空白 + 宣言の名前」（@keyframes の中は「@keyframes 名前 from」が頭） */
const KEEP: Record<string, string> = {
  // 回るだけの印。欠けている辺に向きの意味は無い
  ".bg-spinner border-right-color": "回転する印の欠け。向きの意味が無い",
  // 真ん中に置く（left: 50% と translateX(-50%) の組）。左右どちらから測っても同じ所
  ".viewer-thumb left": "left: 50% で真ん中。左右対称",
  ".speed-toast left": "left: 50% で真ん中。左右対称",
  ".speed-toast transform": "真ん中に置く translateX(-50%)",
  "@keyframes speed-toast-in from transform": "真ん中に置く translateX(-50%)",
  "@keyframes speed-toast-in to transform": "真ん中に置く translateX(-50%)",
  // 読み込み中の帯が流れる向き。RTL では逆に流したいが、情報パネルと一緒に直す
  "@keyframes viewer-progress-slide from transform": "流れる帯の向き（情報パネルと一緒に直す）",
  "@keyframes viewer-progress-slide to transform": "流れる帯の向き（情報パネルと一緒に直す）",
  ".failure-toast left": "left: 50% で真ん中。左右対称",
  ".failure-toast transform": "真ん中に置く translateX(-50%)",
  // 印刷の見本は写真の絵。写真は鏡に映さない
  ".print-fit-sample > span::after left": "印刷の見本の絵の中の丸。写真は鏡に映さない",
  ".print-fit-sample-cover > span::after left": "印刷の見本の絵の中の丸。写真は鏡に映さない",
  ".print-fit-sample-cover > span left": "印刷の見本の絵。写真は鏡に映さない",
  // 情報パネルの側に結びついている。パネルを右から左へ移すときに、
  // 絵の大きさを計算する JS（viewerZoomBox.ts）と一緒に直す
  ".viewer-missing-band left": "情報パネルの側に結びつく（viewerZoomBox と一緒に直す）",
  ".viewer-missing-band transform": "情報パネルの側に結びつく（viewerZoomBox と一緒に直す）",
  ".viewer-flash left": "情報パネルの側に結びつく（viewerZoomBox と一緒に直す）",
};

type Decl = { key: string; prop: string; value: string; sheet: string };

/** コメントを落として、宣言を「直前の規則の頭」と組にして返す。入れ子（@media・@keyframes）も同じ扱い */
function declarations(sheet: string): Decl[] {
  const css = readFileSync(new URL(sheet, src), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const out: Decl[] = [];
  const heads: string[] = [];
  let buf = "";
  for (const ch of css) {
    if (ch === "{") {
      heads.push(buf.trim().replace(/\s+/g, " "));
      buf = "";
    } else if (ch === "}") {
      heads.pop();
      buf = "";
    } else if (ch === ";") {
      const m = /^\s*([-a-z]+)\s*:\s*([\s\S]*)$/.exec(buf);
      if (m) {
        // @keyframes の from/to は名前と組にする（どの動きかが鍵に残るように）
        const head = heads.filter((h) => !h.startsWith("@media")).join(" ");
        out.push({ key: `${head} ${m[1]}`, prop: m[1], value: m[2].trim(), sheet });
      }
      buf = "";
    } else buf += ch;
  }
  return out;
}

/** 4つの値を持つ省略形で、左と右が違うもの（上 右 下 左） */
function asymmetricFour(prop: string, value: string): boolean {
  if (!/^(margin|padding|inset|border-width|border-style|border-color|scroll-margin|scroll-padding)$/.test(prop))
    return false;
  const parts = splitTopLevel(value);
  return parts.length === 4 && parts[1] !== parts[3];
}

/** border-radius の4つの角で、左右の組が違うもの */
function asymmetricRadius(prop: string, value: string): boolean {
  if (prop !== "border-radius") return false;
  const parts = splitTopLevel(value.split("/")[0]);
  if (parts.length < 2) return false;
  const [tl, tr = tl, br = tl, bl = tr] = parts;
  return tl !== tr || bl !== br;
}

/** 空白で分ける。calc(…) の中の空白では切らない */
function splitTopLevel(v: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of v.trim()) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (/\s/.test(ch) && depth === 0) {
      if (cur) out.push(cur);
      cur = "";
    } else cur += ch;
  }
  if (cur) out.push(cur);
  return out;
}

function isPhysical(d: Decl): boolean {
  if (/^(left|right)$/.test(d.prop)) return true;
  if (/^(margin|padding|border|scroll-margin|scroll-padding)-(left|right)(-|$)/.test(d.prop)) return true;
  if (/^border-(top|bottom)-(left|right)-radius$/.test(d.prop)) return true;
  if (/^(text-align|float|clear)$/.test(d.prop) && /\b(left|right)\b/.test(d.value)) return true;
  if (d.prop === "transform" && /translateX\(/.test(d.value)) return true;
  if (d.prop === "translate" && d.value !== "none") return true;
  return asymmetricFour(d.prop, d.value) || asymmetricRadius(d.prop, d.value);
}

const all = SHEETS.flatMap(declarations);

test("left and right are written as logical properties", () => {
  const found = all.filter(isPhysical).filter((d) => !(d.key in KEEP));
  assert.deepEqual(
    found.map((d) => `${d.sheet}: ${d.key}: ${d.value}`),
    [],
    "書き換えるか、KEEP に理由を書く",
  );
});

test("every kept physical property is still there", () => {
  const physical = new Set(all.filter(isPhysical).map((d) => d.key));
  const stale = Object.keys(KEEP).filter((k) => !physical.has(k));
  assert.deepEqual(stale, [], "直したなら KEEP から消す");
});

test("the scanner reads the sheets", () => {
  // 読めていなければ上の2つは空のまま通ってしまう。床を置く
  assert.ok(all.length > 1500, `only ${all.length} declarations`);
  const logical = all.filter((d) => /inline-(start|end)|^inset-inline$|^padding-inline$/.test(d.prop));
  assert.ok(logical.length >= 40, `only ${logical.length} logical declarations`);
});

test("the scanner flags each physical shape", () => {
  const flag = (prop: string, value: string) => isPhysical({ key: "x", prop, value, sheet: "t" });
  assert.ok(flag("left", "0"));
  assert.ok(flag("margin-right", "4px"));
  assert.ok(flag("border-left-color", "red"));
  assert.ok(flag("border-top-left-radius", "4px"));
  assert.ok(flag("text-align", "right"));
  assert.ok(flag("transform", "translateX(-50%)"));
  assert.ok(flag("padding", "0 var(--sp-7) 0 28px"));
  assert.ok(flag("border-radius", "9px 0 0 9px"));
  assert.ok(!flag("padding", "0 calc(1px + 2px) 0 calc(1px + 2px)"));
  assert.ok(!flag("border-radius", "0 0 4px 4px"));
  assert.ok(!flag("margin", "0 auto"));
  assert.ok(!flag("text-align", "start"));
  assert.ok(!flag("inset-inline-start", "0"));
  assert.ok(!flag("transform", "translate(-50%, -50%) scale(1)"));
});
