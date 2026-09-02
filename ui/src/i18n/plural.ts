/**
 * 辞書の中で数を扱うための2つ。**辞書から呼べる唯一の書式**。
 *
 * ## なぜ辞書の中で整形するのか
 *
 * もとは逆だった——**呼ぶ側で `formatNumber(n)` を通してから文字列で渡す**、と
 * `ja.ts` に書いてあった。理由は循環参照で、`index.ts` は辞書を import するので
 * 辞書から `formatLocale` は引けない。
 *
 * **その規約が単複を壊した**（2026-09-02）。整形済みの `string` を受け取った辞書からは
 * **件数が見えない**ので、`n === 1` の場合分けができない。実際
 * `decoderHeifNotice` は独語で `Für 1 HEIC/HEIF-Fotos`、西語で `1 fotos HEIC/HEIF`
 * と出ていた。**単数形は辞書にしか書けないのに、その材料を取り上げていた。**
 *
 * しかも規約が守られていたのは**アプリ全体で3か所だけ**で、数を取る36キーのうち
 * 残り全部が生の `${n}` を埋めていた——独語で12,000枚あると `12000 Fotos`。
 *
 * ## 循環参照は「注入」で解く
 *
 * この file は `index.ts` を **import しない**。逆に `index.ts` が
 * `formatLocale` を決めた直後に `setNumberLocale()` を呼んで**渡してくる**。
 * 辞書が評価される時点では関数を組み立てているだけで、`num()` が実際に走るのは
 * 画面を描くとき——**そのときには `index.ts` は読み終わっている**。
 *
 * `n.toLocaleString()` を裸で呼ぶ道は採らない。**WebViewの既定ロケール**で
 * 整形されるので、`formatLocale`（OSの地域）と**同じ画面で食い違う**
 * ——OSが `es-MX` だと `12,345` と `12.345` が並ぶ。
 */

/**
 * `index.ts` が渡してくるまでは `null`。**そのときは桁区切りを付けない**
 * （`String(n)`）——ここで `toLocaleString()` に落ちると、上に書いた食い違いが
 * 起きたことにさえ気づけない。アプリでは必ず渡ってくるので、この枝に来るのは
 * 辞書だけを単体で読んだとき（`ui/scripts/i18n.test.ts`）。
 */
let format: Intl.NumberFormat | null = null;

/** 桁区切りに使うロケールを渡す。`index.ts` が `formatLocale` を決めた直後に1度だけ呼ぶ */
export function setNumberLocale(tag: string): void {
  try {
    format = new Intl.NumberFormat(tag);
  } catch {
    // 綴りを受け付けないロケール。桁区切り無しで出す（画面は落とさない）
    format = null;
  }
}

/**
 * 件数などの数値。**辞書の中の `${n}` は、原則これを通す。**
 *
 * **通さないのは暦の年だけ**——`jumpToYear(2019)` を通すと独語で `2.019` になる。
 */
export const num = (n: number): string => (format ? format.format(n) : String(n));

/**
 * **数詞1のあとは単数**（2026-09-01、遡ってのゲート2）。`1 Dateien` は目に付く。
 * スペイン語は分詞まで性数が一致するので、`1 movidas` のように
 * **1枚消すたびに文法が崩れる**。
 *
 * 独語・西語それぞれの辞書に同じものが書いてあったのを、ここへ出した
 * （3つ目の言語が写す前に片付ける）。**日本語・中国語は単複が無いので使わない。**
 *
 * 英語も要る——`full scan (1 files)` は誰も直していなかった。
 */
export const one = (n: number, singular: string, plural: string): string =>
  n === 1 ? singular : plural;
