/**
 * 画面の向き（右から左へ書く言語）。
 *
 * **向きは `<html dir>` が持つ**（main.tsx が言語から決めて付ける）。JS はそれを**その都度読む**——
 * 起動時に控えた値を持たないので、試験で `dir` を付け替えても追える。CSS は論理プロパティと
 * `[dir="rtl"]` で同じ属性に従う（scripts/logicalCss.test.ts）。`:dir()` は使わない——WebKit 16.4 より
 * 前（macOS 11・12 に残りうる）では効かず、鏡が半分だけになる
 */

/**
 * 右から左へ書く言語。アラビア語・ヘブライ語・ペルシア語・ウルドゥー語のほか、同じ文字を使う言語。
 * **文字の札（`-Arab` / `-Latn` など）が付いていれば、言語より札を信じる**——`pa-Arab` は右→左、
 * `sd-Deva`・`ar-Latn` は左→右。エンジンの `Intl.Locale` の textInfo には頼らない（WebView2 と WebKit で
 * 載っている版が違い、同じ言語で向きが割れうる）
 */
// iw・ji はヘブライ語・イディッシュ語の古いコード（Java 由来の環境がまだ返す）。prs はダリー語（Windows が返す）
const RTL_LANGS = /^(ar|he|iw|fa|prs|ur|ckb|ps|sd|yi|ji|dv|ug|ks|syr|mzn|lrc|nqo|bal|pnb|azb)$/i;
const RTL_SCRIPTS = /^(Arab|Hebr|Thaa|Syrc|Nkoo|Adlm|Rohg|Mand|Samr)$/i;

export function dirOf(locale: string): "rtl" | "ltr" {
  const parts = locale.split(/[-_]/);
  // 文字の札は言語（と拡張言語 3 文字）の直後だけ。`-u-nu-latn` のような拡張の中の 4 文字は見ない
  let at = 1;
  while (at < parts.length && /^[A-Za-z]{3}$/.test(parts[at]) && at < 4) at++;
  const script = /^[A-Za-z]{4}$/.test(parts[at] ?? "") ? parts[at] : undefined;
  const rtl = script ? RTL_SCRIPTS.test(script) : RTL_LANGS.test(parts[0]);
  return rtl ? "rtl" : "ltr";
}

/** 横の向きの符号。左→右は 1、右→左は -1——CSS の `--inline-sign` と同じ。画面の座標を向きで裏返すときに掛ける */
export const inlineSign = (): 1 | -1 => (isRtl() ? -1 : 1);

/** 右→左の文字（ヘブライ文字・アラビア文字と、その表示形）を含むか */
export const hasRtlText = (s: string): boolean =>
  // 表示形は U+FEFC まで（U+FEFF は BOM・幅の無い空白なので数えない）
  /[\u0590-\u08ff\ufb1d-\ufdcf\ufdf0-\ufdff\ufe70-\ufefc]/u.test(s);

/**
 * **数字を ASCII に揃える**（アラビア・インド数字 `٠-٩`、ペルシア数字 `۰-۹`）。アラビア語の配列は
 * 数字の列からこれを送るので、検索の日付（`٢٠١٩-٠٨`）がファイル名の検索に落ちないように、
 * 渡す前に揃える。検索語には下の `asciiDateTokens()` を使う
 */
export const asciiDigits = (s: string): string =>
  s.replace(/[\u0660-\u0669\u06f0-\u06f9]/gu, (d) => {
    const c = d.charCodeAt(0);
    return String(c >= 0x06f0 ? c - 0x06f0 : c - 0x0660);
  });

/** いまの画面が右→左か */
export const isRtl = (): boolean =>
  typeof document !== "undefined" && document.documentElement.dir === "rtl";

/**
 * 画面の左右（矢印キー）を、読む順の前後へ。**右→左では左が「次」**——本の頁をめくる向きと同じ。
 * 送りのボタン（‹ ›）も論理の位置に置いてあるので、キーとボタンの向きは揃う
 */
export const stepOfArrow = (key: "ArrowLeft" | "ArrowRight", rtl: boolean): -1 | 1 =>
  (key === "ArrowRight") !== rtl ? 1 : -1;

/**
 * 検索語のうち、**日付として読まれる語**の数字だけを揃える——素の数と区切りだけの語（`٢٠١٩`・`٢٠١٩-٠٨`）と、
 * 年の指定（`year:٢٠١٩`・`年:٢٠١٩`）。素の数だけの語は ASCII の利用者と同じく日付として読まれる
 * （`trip 2019` と同じ結果）。**それ以外は打ったまま**——名前に混ざった数字（`رحلة٢٠١٩`・`DSC٠٠٠٨٩`）、
 * 名前を指す欄（`folder:٢٠١٩`・`camera:…`。`search.rs` は字のまま探す）、引用符の中（`"trip ٢٠١٩"`）
 */
export const asciiDateTokens = (q: string): string =>
  // 引用符で括った値（`"…"`・`folder:"…"`）は空白を含んでも1語として扱う
  (q.match(/[^\s"]*"[^"]*"?|\S+|\s+/gu) ?? [])
    .map((tok) => (/^((year|年):)?[\p{Nd}\-/.]+$/iu.test(tok) ? asciiDigits(tok) : tok))
    .join("");

/**
 * **向きの閉じ（FSI/LRI/RLI … PDI）を、古い埋め込み（LRE/RLE … PDF）に置き換える**。
 * Windows の TaskDialog は閉じの印を解さず、点線の箱で描いたうえ、アラビア語で終わる名前を
 * 周りの文へ混ぜた（#283 の win の実機）。埋め込みは古い描き手でも効く。
 * FSI の向きは、中の最初の文字（字）で決める——Unicode の FSI と同じ決め方。字が無ければ左から右
 */
export function isolatesToEmbeddings(s: string): string {
  const chars = [...s];
  return chars
    .map((c, i) => {
      if (c === "\u2066") return "\u202a";
      if (c === "\u2067") return "\u202b";
      if (c === "\u2069") return "\u202c";
      if (c !== "\u2068") return c;
      let depth = 0;
      for (const d of chars.slice(i + 1)) {
        if (d === "\u2066" || d === "\u2067" || d === "\u2068") depth++;
        else if (d === "\u2069") {
          if (depth === 0) break;
          depth--;
        } else if (depth === 0 && /\p{L}/u.test(d)) {
          return hasRtlText(d) ? "\u202b" : "\u202a";
        }
      }
      return "\u202a";
    })
    .join("");
}
