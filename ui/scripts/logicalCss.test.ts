/**
 * 右から左へ書く言語（アラビア語・ヘブライ語）に備えて、画面の左右は**論理プロパティ**で書く。
 * `margin-left` ではなく `margin-inline-start`、`right` ではなく `inset-inline-end`。
 * 横書きの左→右では物理と同じ値になるので、いまの見た目は変わらない。`dir="rtl"` で鏡に映る。
 *
 * 物理のまま残すものは下の `KEEP` に**値と理由ごと**書く。`App.css` と `tokens.css` に
 * 新しく左右を書いたら、ここで落ちる。部品の `style={{ left }}` は見ない——そこに在るのは
 * 押した所・絵の箱といった**画面の座標**で、向きではない。
 *
 * 論理の角丸（`border-start-start-radius`）は WebKit 15 から。画面は既に `oklch()`（15.4〜）で
 * 色を塗っているので、それより古い WebKit では元から描けない。
 * `npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import postcss from "postcss";

const src = new URL("../src/", import.meta.url);
const SHEETS = ["App.css", "tokens.css"];

/**
 * ビューアの左右。情報パネルは右に開き、絵の大きさは JS（viewerZoomBox.ts）も右にパネルが
 * ある前提で計算する。送り（← →）・戻る（←）・道具の帯も、その向きと組で決まっている。
 * **一部だけ鏡に映ると、右→左で絵と合図と送りがずれる**ので、`.viewer` を `direction: ltr` で
 * まるごと留めてある。パネルを左へ移すときに、JS・矢印キー・印の向きと一緒にまとめて外す。
 */
const VIEWER = "ビューアは direction: ltr に留めてある（パネル・viewerZoomBox・送りと一緒に外す）";
const CENTRE = "left: 50% と translateX(-50%) で真ん中。左右どちらから測っても同じ所";
const PICTURE = "印刷の見本の絵。写真は鏡に映さない";
const SPINNER = "回転する印の欠け。向きの意味が無い";
const SHIMMER = "読み込み中の光が流れる向き。飾りで、どちら向きでも読める";
const QUIET = "@media (prefers-reduced-motion: reduce) @keyframes viewer-flash-quiet";

/** 物理のまま残すもの。鍵は「規則の頭（@media・@keyframes も含む）|宣言|値」 */
const KEEP: Record<string, string> = {
  ".bg-spinner|border-right-color|transparent": SPINNER,
  "@media (prefers-reduced-motion: reduce) .bg-spinner|border-right-color|currentColor": SPINNER,
  ".wiz-node.wiz-card|box-shadow|inset 3px 0 0 var(--accent)": "影に論理の形が無い。右→左は [dir=rtl] で裏返す",
  '[dir="rtl"] .wiz-node.wiz-card|box-shadow|inset -3px 0 0 var(--accent)': "上の裏返し",
  ".placeholder-row|background|linear-gradient( 90deg, var(--cell-pending) 0 60%, var(--surface) 80%, var(--cell-pending) 100% ) 0 0 / 200% 100%":
    SHIMMER,
  "@keyframes placeholder-shimmer from|background-position|0 0": SHIMMER,
  "@keyframes placeholder-shimmer to|background-position|-200% 0": SHIMMER,

  ".viewer-thumb|left|50%": CENTRE,
  ".speed-toast|left|50%": CENTRE,
  ".speed-toast|transform|translateX(-50%)": CENTRE,
  "@keyframes speed-toast-in from|transform|translateX(-50%) translateY(8px)": CENTRE,
  "@keyframes speed-toast-in to|transform|translateX(-50%) translateY(0)": CENTRE,
  ".failure-toast|left|50%": CENTRE,
  ".failure-toast|transform|translateX(-50%)": CENTRE,

  ".print-fit-sample > span::after|left|18%": PICTURE,
  ".print-fit-sample-cover > span::after|left|6%": PICTURE,
  ".print-fit-sample-cover > span|left|-50%": PICTURE,

  ".viewer-stage|right|var(--info-reserve)": VIEWER,
  ".viewer-loading-overlay|right|var(--info-reserve)": VIEWER,
  ".viewer-missing-band|left|calc((100vw - var(--info-reserve)) / 2)": VIEWER,
  ".viewer-missing-band|transform|translateX(-50%)": VIEWER,
  ".viewer-caption|left|0": VIEWER,
  ".viewer-caption|right|var(--info-reserve)": VIEWER,
  ".viewer-strip|left|0": VIEWER,
  ".viewer-strip|right|var(--info-reserve)": VIEWER,
  ".viewer-tools|right|var(--sp-4)": VIEWER,
  ".viewer-back|left|var(--sp-4)": VIEWER,
  ".viewer-badges|left|var(--sp-4)": VIEWER,
  ".viewer-flash|left|calc((100% - var(--info-reserve)) / 2)": VIEWER,
  "@keyframes viewer-flash 0%|transform|translate(-50%, -50%) scale(0.7)": VIEWER,
  "@keyframes viewer-flash 18%|transform|translate(-50%, -50%) scale(1)": VIEWER,
  "@keyframes viewer-flash 70%|transform|translate(-50%, -50%) scale(1)": VIEWER,
  "@keyframes viewer-flash 100%|transform|translate(-50%, -50%) scale(1.08)": VIEWER,
  [`${QUIET} 0%, 70%|transform|translate(-50%, -50%)`]: VIEWER,
  [`${QUIET} 100%|transform|translate(-50%, -50%)`]: VIEWER,
  "@keyframes viewer-progress-slide from|transform|translateX(-100%)": VIEWER,
  "@keyframes viewer-progress-slide to|transform|translateX(360%)": VIEWER,
  ".viewer-nav.prev|left|var(--sp-4)": VIEWER,
  ".viewer-nav.next|right|calc(var(--info-reserve) + var(--sp-4))": VIEWER,
  ".exif-panel|right|var(--sp-4)": VIEWER,
};

type Decl = { key: string; prop: string; value: string };

/** 宣言を「囲んでいる規則の頭（@media・@keyframes も含む）|宣言|値」の鍵と組にして返す */
function parse(text: string): Decl[] {
  const out: Decl[] = [];
  postcss.parse(text).walkDecls((d) => {
    const heads: string[] = [];
    for (let n: postcss.Container | postcss.Document | undefined = d.parent; n && n.type !== "root"; n = n.parent) {
      if (n.type === "rule") heads.unshift((n as postcss.Rule).selector.replace(/\s+/g, " "));
      else if (n.type === "atrule") heads.unshift(`@${(n as postcss.AtRule).name} ${(n as postcss.AtRule).params}`);
    }
    const value = d.value.replace(/\s+/g, " ").trim();
    out.push({ key: `${heads.join(" ")}|${d.prop}|${value}`, prop: d.prop, value });
  });
  return out;
}

/** 区切り（既定は空白）で分ける。calc(…) の中では切らない */
function splitTopLevel(v: string, sep = /\s/): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of v.trim()) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (sep.test(ch) && depth === 0) {
      if (cur.trim()) out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

const zero = (v: string | undefined) => v === undefined || /^-?0(\.0+)?([a-z%]+)?$/.test(v);
/** 位置の横の値で、左右どちらの端にも寄らないもの。`0` は左端なので入らない */
const middle = (v: string) => v === "50%" || v === "center";
const VERTICAL = /^(top|bottom|center)$/;

/** 4つの値を持つ省略形で、左と右が違うもの（上 右 下 左） */
function asymmetricFour(prop: string, value: string): boolean {
  if (!/^(margin|padding|inset|border-width|border-style|border-color|scroll-margin|scroll-padding)$/.test(prop))
    return false;
  const parts = splitTopLevel(value);
  return parts.length === 4 && parts[1] !== parts[3];
}

/** border-radius の4つの角で、左右の組が違うもの（`/` の後ろの縦の半径も見る） */
function asymmetricRadius(prop: string, value: string): boolean {
  if (prop !== "border-radius") return false;
  return splitTopLevel(value, /\//).some((half) => {
    const parts = splitTopLevel(half);
    if (parts.length < 2) return false;
    const [tl, tr = tl, br = tl, bl = tr] = parts;
    return tl !== tr || bl !== br;
  });
}

/** transform の中で、横へ動かすか、左右を裏返すもの */
function movesSideways(value: string): boolean {
  if (/translateX\(|translate3d\(|matrix(3d)?\(|skewX\(|scaleX\(\s*-/.test(value)) return true;
  for (const m of value.matchAll(/translate\(([^)]*)\)/g)) {
    if (!zero(splitTopLevel(m[1], /,/)[0])) return true;
  }
  for (const m of value.matchAll(/scale\(([^)]*)\)/g)) {
    if (splitTopLevel(m[1], /,/)[0].startsWith("-")) return true;
  }
  return false;
}

/** 位置（background-position 等）の層ごとの横の値が、真ん中か端の 0 以外なら向きを持つ */
function sidewaysPosition(value: string): boolean {
  return splitTopLevel(value, /,/).some((layer) => {
    if (/\b(left|right)\b/.test(layer)) return true;
    const parts = splitTopLevel(layer);
    // 1語だけの top / bottom は縦の話
    if (parts.length === 1 && VERTICAL.test(parts[0])) return false;
    return !middle(parts[0]);
  });
}

/** グラデーションが横向きのもの（真上・真下以外の角度、to left/right、at left/right） */
function sidewaysGradient(value: string): boolean {
  if (/\b(to|at)\s+(\w+\s+)?(left|right)\b/.test(value)) return true;
  if (/conic-gradient\(/.test(value)) return true;
  for (const m of value.matchAll(/gradient\(\s*(-?\d*\.?\d+)(deg|turn|rad|grad)/g)) {
    const turns = { deg: 1 / 360, turn: 1, rad: 1 / (2 * Math.PI), grad: 1 / 400 }[m[2] as "deg"];
    const t = ((Number(m[1]) * turns) % 0.5 + 0.5) % 0.5;
    // 135deg のような斜めは飾りの光沢で、どちらに傾いても読める。真横（0.25 turn）だけを向きとみなす
    if (Math.abs(t - 0.25) < 1e-9) return true;
  }
  return false;
}

/** 影の層ごとの横のずれ（inset を除いた最初の長さ）が 0 でないもの */
function sidewaysShadow(value: string): boolean {
  if (value === "none") return false;
  return splitTopLevel(value, /,/).some((layer) => {
    const lengths = splitTopLevel(layer).filter((p) => /^-?\d*\.?\d+([a-z]+)?$/.test(p) || /^calc\(/.test(p));
    return lengths.length >= 2 && !zero(lengths[0]);
  });
}

function isPhysical(d: Pick<Decl, "prop" | "value">): boolean {
  const { prop, value } = d;
  if (/^(left|right)$/.test(prop)) return true;
  if (/^(margin|padding|border|scroll-margin|scroll-padding)-(left|right)(-|$)/.test(prop)) return true;
  if (/^border-(top|bottom)-(left|right)-radius$/.test(prop)) return true;
  if (/^(text-align|float|clear|justify-content|justify-items|justify-self)$/.test(prop))
    return /\b(left|right)\b/.test(value);
  if (/^(background-position|background-position-x|object-position|transform-origin|perspective-origin)$/.test(prop))
    return sidewaysPosition(value);
  if (/^(box-shadow|text-shadow)$/.test(prop) && sidewaysShadow(value)) return true;
  if (/gradient\(/.test(value) && sidewaysGradient(value)) return true;
  if (prop === "background" && /\b(left|right)\b/.test(value.replace(/gradient\([^)]*\)/g, ""))) return true;
  if (prop === "transform") return movesSideways(value);
  if (prop === "translate") return value !== "none" && !zero(splitTopLevel(value)[0]);
  if (prop === "scale") return splitTopLevel(value)[0]?.startsWith("-") ?? false;
  return asymmetricFour(prop, value) || asymmetricRadius(prop, value);
}

const all = SHEETS.flatMap((s) => parse(readFileSync(new URL(s, src), "utf8")).map((d) => ({ ...d, sheet: s })));

test("left and right are written as logical properties", () => {
  const found = all.filter(isPhysical).filter((d) => !(d.key in KEEP));
  assert.deepEqual(
    found.map((d) => `${d.sheet}: ${d.key}`),
    [],
    "書き換えるか、KEEP に値と理由を書く",
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
  assert.ok(logical.length >= 30, `only ${logical.length} logical declarations`);
});

test("the scanner flags each physical shape", () => {
  for (const [prop, value] of [
    ["left", "0"],
    ["margin-right", "4px"],
    ["border-left-color", "red"],
    ["border-top-left-radius", "4px"],
    ["text-align", "right"],
    ["transform", "translateX(-50%)"],
    ["transform", "translate(12px, 0)"],
    ["transform", "translate(-50%, -50%) scale(1)"],
    ["transform", "matrix(1, 0, 0, 1, 4, 0)"],
    ["translate", "12px"],
    ["transform-origin", "left center"],
    ["background-position", "-200% 0"],
    ["object-position", "right"],
    ["background-position", "center, 100% 0"],
    ["background-position", "0 0, right 4px top"],
    ["transform-origin", "0 0"],
    ["transform", "scale(-1, 1)"],
    ["scale", "-1 1"],
    ["box-shadow", "inset 3px 0 0 red"],
    ["text-shadow", "0 1px 2px red, -1px 0 red"],
    ["background", "linear-gradient(180deg, a, b), linear-gradient(90deg, c, d)"],
    ["background", "linear-gradient(0.25turn, a, b)"],
    ["background", "radial-gradient(circle at left, a, b)"],
    ["background", "url() no-repeat right 8px center"],
    ["border-radius", "calc(var(--r) / 2) 0 0 calc(var(--r) / 2)"],
    ["background", "linear-gradient(to right, red, blue)"],
    ["background", "linear-gradient(90deg, red, blue)"],
    ["padding", "0 var(--sp-7) 0 28px"],
    ["border-radius", "9px 0 0 9px"],
    ["border-radius", "4px / 9px 0 0 9px"],
  ]) {
    assert.ok(isPhysical({ prop, value }), `${prop}: ${value}`);
  }
  for (const [prop, value] of [
    ["padding", "0 calc(1px + 2px) 0 calc(1px + 2px)"],
    ["border-radius", "0 0 4px 4px"],
    ["margin", "0 auto"],
    ["text-align", "start"],
    ["inset-inline-start", "0"],
    ["transform", "translate(0, -50%)"],
    ["transform", "translateY(-50%) scale(1.1)"],
    ["translate", "0 -50%"],
    ["background-position", "center"],
    ["background", "linear-gradient(135deg, red, blue)"],
    ["background", "linear-gradient(180deg, red, blue)"],
    ["object-position", "top"],
    ["transform-origin", "center"],
    ["transform-origin", "50% 0"],
    ["text-shadow", "0 1px 3px oklch(0 0 0 / 0.8)"],
    ["box-shadow", "inset 0 0 0 1px var(--warn)"],
    ["box-shadow", "var(--shadow-2)"],
    ["background", "linear-gradient(0.5turn, a, b)"],
  ]) {
    assert.ok(!isPhysical({ prop, value }), `${prop}: ${value}`);
  }
});

test("the scanner keeps the last declaration and ignores braces in strings", () => {
  assert.deepEqual(
    parse('.a { content: "{"; left: 0 } @media (x) { .b { top: 1px } }').map((d) => d.key),
    ['.a|content|"{"', ".a|left|0", "@media (x) .b|top|1px"],
  );
});

test("the viewer stays pinned left-to-right while its rules are kept physical", () => {
  // VIEWER の物理が許されるのは、.viewer を direction: ltr で留めている間だけ
  const pinned = all.some((d) => d.key === ".viewer|direction|ltr");
  const kept = Object.values(KEEP).includes(VIEWER);
  assert.equal(pinned, kept, pinned ? "VIEWER が空なら direction: ltr も外す" : "direction: ltr を外すなら VIEWER の物理も直す");
});
