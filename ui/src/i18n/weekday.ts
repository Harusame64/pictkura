/**
 * 週の始まりを、**OSで効いている値**として受け取る。
 *
 * ## なぜ `Intl` では足りないのか
 *
 * 桁区切りも日付の並びも地域のタグ（`formatLocale`）で正しくなるが、
 * **週の始まりだけはタグに映らない**。Windowsの明示設定を 0/3/6 と振っても
 * ロケール名は `ja-JP` のまま動かず、`Intl` はタグだけで答えるので、
 * **木曜始まりにしている人が既定の日曜に見える**（2026-09-04、実測。dev #18 残件(b)）。
 *
 * だから `src-tauri/src/lib.rs` の `os_first_weekday()` が
 * **効いている値を読んで**（Windowsは `LOCALE_IFIRSTDAYOFWEEK`、macOSは
 * `CFCalendarGetFirstWeekday`）、`window.__PICTKURA_FIRST_WEEKDAY__` に置く。
 *
 * ## なぜ別ファイルなのか
 *
 * `index.ts` は `window` と `localStorage` を読むのでNodeのテストから読めない。
 * **ここは受け取った値を検める関数だけ**にしてあるので、`i18n.test.ts` が直に呼べる。
 * 守りたいのは下の原点の話で、それは目で見るより機械に見せたほうが確実である。
 *
 * ## 原点が3つあって、どれも一致しない
 *
 * | | 月曜 | 日曜 |
 * |---|---|---|
 * | Win32 `LOCALE_IFIRSTDAYOFWEEK` | 0 | 6 |
 * | CoreFoundation | 2 | 1 |
 * | `Date.getDay()` | 1 | 0 |
 *
 * **渡ってくるのは `Date.getDay()` の原点**（Rust側で揃えている）。
 * `weekdayLabels` と `Calendar.tsx` はこの数をそのまま添字に使うので、
 * **他の原点の値が素通しで届くと、日曜始まりや土曜始まりのカレンダーが出る**
 * ——土曜始まりはどの地域の習慣でもない。
 */

/**
 * 置かれた値が使えるものなら返す。**そうでなければ `null`**（呼び出し側が `Intl` へ倒す）。
 *
 * 弾くのは、Rustが値を置く前の版・Rustの居ない開発中のブラウザ（`undefined`）と、
 * **範囲の外**。`0` は日曜という正しい値なので、`if (raw)` のような真偽の判定にしない
 * ——日曜始まりの人だけ既定へ落ちる、という気づきにくい壊れ方になる。
 */
export function firstWeekdayFromOs(raw: unknown): number | null {
  return typeof raw === "number" && Number.isInteger(raw) && raw >= 0 && raw <= 6
    ? raw
    : null;
}
