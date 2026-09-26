/**
 * 重ねたタイルの右下の印を、**タイルの幅に入る形**で選ぶ（dev #32。純関数。
 * `scripts/stackBadges.test.ts` が表で見る）。
 *
 * 紙を描かなくなってから（2026-09-26 の利用者の選択「E」）、印は重ねの唯一の目印なので、
 * **切って見せない**。切ると `RAW+JPEG` が数文字になり、連写のコマ数は `▤ 1,234` が `234` に
 * 見える——欠けたと分からない、もっともらしい別の数になる（#167 のゲート2）。
 *
 * 形は幅で選ぶ。CSS の `@container` ではなくここで選ぶのは、macOS 11 の WebKit が
 * コンテナクエリを知らないため（#167 の codex とゲート2）。タイルの幅は描くときに分かっている。
 *
 * 候補を好ましい順に並べ、**入る最初の1つ**を出す。どれも入らなければ最後の候補
 * （数を出さない `▤` だけ）——誤った数を見せるより、数を出さない。
 * 組の短い形は四角が2枚重なった記号（2026-09-26 の利用者の選択。広いタイルは文字のまま）。
 */

/** 印の1つ。`pair-text` は `RAW+JPEG`、`pair-icon` は記号、`burst` は連写の文字 */
export type BadgePart =
  | { kind: "pair-text" }
  | { kind: "pair-icon" }
  | { kind: "burst"; text: string };

export interface BadgeLabels {
  /** 組（RAW+JPEG）か */
  pair: boolean;
  /** 連写なら、言葉つきの形（`▤ 連写 12`）と短い形（`▤ 12`）。連写でなければ無い */
  burst?: { long: string; short: string };
}

export interface BadgeChoice {
  parts: BadgePart[];
  /** 余白を詰めるか（印の左右 3px・間 2px。詰めないときは 5px・3px） */
  tight: boolean;
}

/** 組の文字 */
export const PAIR_TEXT = "RAW+JPEG";
/** 連写の、数を出さない最後の形 */
export const BURST_GLYPH = "▤";
/** 組の記号の幅（App.tsx の SVG と合わせる） */
export const PAIR_ICON_WIDTH = 13;

const PAD = { normal: 5, tight: 3 };
const GAP = { normal: 3, tight: 2 };

/**
 * `room` はタイルの中で印に使える幅（px）。`measure` は文字の幅（px）を返す
 * ——画面では canvas で測り、試験では固定の幅を渡す
 */
export function chooseBadges(
  room: number,
  labels: BadgeLabels,
  measure: (text: string) => number,
): BadgeChoice {
  const pairText: BadgePart = { kind: "pair-text" };
  const pairIcon: BadgePart = { kind: "pair-icon" };
  const b = labels.burst;
  const candidates: BadgeChoice[] = [];
  const add = (parts: BadgePart[], tight: boolean) => candidates.push({ parts, tight });
  if (labels.pair && b) {
    const short: BadgePart = { kind: "burst", text: b.short };
    add([pairText, short], false);
    add([pairIcon, short], false);
    add([pairIcon, short], true);
    // 組の記号を外してでもコマ数を残す（数は記号より情報が多い。組はツールチップが言う）
    add([short], true);
    add([{ kind: "burst", text: BURST_GLYPH }], true);
  } else if (labels.pair) {
    add([pairText], false);
    add([pairIcon], false);
    add([pairIcon], true);
  } else if (b) {
    add([{ kind: "burst", text: b.long }], false);
    add([{ kind: "burst", text: b.short }], false);
    add([{ kind: "burst", text: b.short }], true);
    add([{ kind: "burst", text: BURST_GLYPH }], true);
  }
  if (candidates.length === 0) return { parts: [], tight: false };
  const widthOf = (c: BadgeChoice) => {
    const pad = c.tight ? PAD.tight : PAD.normal;
    const gap = c.tight ? GAP.tight : GAP.normal;
    const inner = c.parts.map((p) =>
      p.kind === "pair-text" ? measure(PAIR_TEXT) : p.kind === "pair-icon" ? PAIR_ICON_WIDTH : measure(p.text),
    );
    return inner.reduce((sum, w) => sum + w + 2 * pad, 0) + gap * (c.parts.length - 1);
  };
  return candidates.find((c) => widthOf(c) <= room) ?? candidates[candidates.length - 1];
}
