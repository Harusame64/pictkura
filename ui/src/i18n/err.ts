/**
 * Rust から届いた失敗を、**画面の言葉**にする（週の台紙の項目3）。
 *
 * ## 何を受け取るか
 *
 * Rust 側は `errs.rs` の約束で、**辞書の鍵**（`ja.ts` のキーそのもの）と
 * **詳細**を制御文字 `U+0001` で継いだ文字列を返す。例:
 * `errTrashFailed` + `U+0001` + `D:\Camera\IMG_0001.xmp: アクセスが拒否されました (os error 5)`。
 *
 * **詳細にパスが入ることがある。** `errTrashFailed` は 0.2.8 から
 * **転んだファイルの名前を先頭に**持つ——`trash` クレートが返すのは
 * `Some operations were aborted` だけで、**どれが・なぜ、を持っていない**
 * （2026-09-07・Windows 実機）。写真は消えて `.xmp` だけが残るのに、
 * **一覧に出ないので辿れなかった。**
 *
 * ## 何をしないか
 *
 * - **詳細は訳さない。** OSの文言・パス・SQLite の理由は、こちらの言葉ではない。
 *   **訳せないものを訳したふりをしない**
 * - **詳細は「OSの言語」で来る**（利用者の言語ではない）。日本語のWindowsで
 *   英語の画面を使っている人には、**文は英語・詳細だけ日本語**という行が出うる。
 *   **承知のうえで出す**——「アクセスが拒否されました」は、隠すより見えたほうが
 *   次の手が分かる。**詳細が付く鍵は4つだけ**で（`errDb` / `errConfigIo` /
 *   `errConfigFormat` / `errTrashFailed`）、残りはパス・枚数・詳細なしである
 *   （`errTrashFailed` の詳細は**パスとOSの文言の両方**）。
 *   **画面を1言語に揃えたいなら、その4つの詳細を記録だけへ回す**——
 *   `errs.rs` は既にどちらか知っている（別の判断として立てる）
 * - **知らない文字列はそのまま返す。** 鍵の付いていない道はまだ残っている
 *   （2026-09-07 時点で Rust 側に27か所。**どれも外のクレートやOSの文言**で、
 *   こちらが書いた日本語は残っていない——ゲート2が3か所の見落としを見つけて
 *   潰したあとの数である）。**画面から文字を消すより、生のまま出すほうがまし**
 */

import { t } from "./index.ts";

/** 鍵と詳細の継ぎ目（`src-tauri/src/errs.rs` の `SEP` と同じ字）。 */
const SEP = "\u0001";

/**
 * 失敗を1行にする。
 *
 * 辞書の値が関数のときは**詳細を渡して呼ぶ**（枚数のような、文に織り込む値）。
 * 文字列のときは、詳細があれば後ろへ添える。
 *
 * `fallback` は**鍵が引けなかったときだけ**使う。外のクレートの文言より
 * こちらの1文のほうが読める場面（「ログを開く」が開けなかった、など）で渡す。
 */
export function errText(e: unknown, fallback?: string): string {
  const raw = String(e);
  const cut = raw.indexOf(SEP);
  const code = cut === -1 ? raw : raw.slice(0, cut);
  const detail = cut === -1 ? "" : raw.slice(cut + 1);

  // **自前の鍵だけを見る。** `[code]` はプロトタイプ鎖まで歩くので、
  // `toString` や `constructor` という語が鍵の位置に来ると
  // **`Object.prototype` の関数を呼んでしまう**（`index.ts` が `DICTS` で
  // 同じ罠を名指ししている・ゲート2）
  const dict = t as unknown as Record<string, unknown>;
  const say = Object.prototype.hasOwnProperty.call(dict, code)
    ? dict[code]
    : undefined;
  // **数だけの詳細は数として渡す。** 辞書の側は `num()` に通して桁を区切るので
  // （`i18n.test.ts` が6言語ぶん見ている）、文字列のまま渡すと
  // **12,345 が 12345 になる**——その1点のためだけの変換である
  if (typeof say === "function") {
    const arg = /^\d+$/.test(detail) ? Number(detail) : detail;
    return String(say(arg));
  }
  if (typeof say !== "string") {
    // **知らない鍵**（辞書に無い・別のモジュールが付けた・打ち間違い）。
    // **生のまま返すが、継ぎ目は開く**——`U+0001` は画面に出ない字なので、
    // そのままだと鍵と詳細が**空白も句読点も無しに溶接されて**出る（ゲート2）
    const plain = cut === -1 ? raw : `${code} \u2014 ${detail}`;
    // 呼ぶ側が**その場に合った文**を持っているならそちらを優先する
    // （外のクレートの文言より、こちらの1文のほうが読める場面がある）
    return fallback ?? plain;
  }
  return detail ? `${say} \u2014 ${detail}` : say;
}
