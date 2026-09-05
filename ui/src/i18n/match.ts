/**
 * **OSの言語タグから、どの辞書を使うかを決める。** ここだけが規則を持つ。
 *
 * ## なぜ `index.ts` から出したのか
 *
 * `index.ts` は `window` と `localStorage` を読むので、**Nodeのテストから呼べない**。
 * この梯子は**これまで両ゲートに4回直されている**（繁体字を簡体字へ落とす・
 * 広東語が英語まで落ちる・書き言葉より地域を先に見る・`es-MX` が本国へ着く）のに、
 * **機械で確かめる手が無かった**。ここは `window` を触らないので、
 * `i18n.test.ts` が直に呼んで表で確かめられる。
 *
 * `weekday.ts` を分けたのと同じ理由である。
 */

/**
 * タグの地域副タグ。**2文字＝国**（`mx`）、**3桁＝国連の地域コード**（`419`）。
 *
 * 位置で決め打ちしない——`es-Latn-MX` のように**書き言葉が挟まる**ことがある。
 *
 * **1文字の副タグが出たらそこで止める**（ゲート1）。BCP-47 では1文字は**拡張の始まり**で、
 * その先は言語の話ではない——`es-u-nu-latn` の `nu`、`es-u-hc-h23` の `hc` は
 * **2文字だが地域ではない**。止めないと、地域を持たないタグが中南米へ落ちる。
 */
export function regionOf(parts: string[]): string | null {
  for (const p of parts.slice(1)) {
    if (p.length === 1) return null;
    if (/^[a-z]{2}$/.test(p) || /^[0-9]{3}$/.test(p)) return p;
  }
  return null;
}

/**
 * その言語タグが繁体字か。
 *
 * **書き言葉が書いてあればそれに従い、地域は書いていないときだけ見る**
 * （ゲート1の指摘）。順番を逆にすると **`zh-Hans-HK` / `zh-Hans-MO`**
 * ——香港・マカオで簡体字を選んでいる人——が地域だけで繁体字と判定され、
 * `zh` の辞書を飛ばして**英語まで落ちる**。`Hans` と書いてあるのだから、
 * 地域から推し量る必要はない。
 *
 * 地域を見るのは `zh-TW` / `zh-HK` のように書き言葉が省かれたときだけ。
 * **CLDRの表を写しているのではない**——繁体字の地域は3つで尽きていて増減しない。
 */
export function isTraditionalChinese(parts: string[]): boolean {
  if (parts.includes("hans")) return false;
  if (parts.includes("hant")) return true;
  return parts.some((p) => p === "tw" || p === "hk" || p === "mo");
}

/**
 * 優先順のタグから、**持っている辞書のコード**を1つ選ぶ。無ければ `null`。
 *
 * `hasDict` を渡してもらうのは、**辞書の顔ぶれを知っているのは `index.ts` だけ**だから
 * ——ここに辞書を持ち込むと、この関数がテストから呼べなくなる（`ja.ts` を読むと
 * その先で `folderExample.ts` まで引く）。
 */
export function matchLocale(tags: string[], hasDict: (code: string) => boolean): string | null {
  for (const tag of tags) {
    // **後ろから1つずつ短くして探す**——`zh-Hant-TW` → `zh-hant` → `zh`、
    // `ja-JP` → `ja`。地域を落とすだけだと**真ん中の副タグを飛ばす**ので、
    // `zh-hant.ts` を足しても `zh-Hant-TW` の人には当たらない
    const parts = tag.toLowerCase().split("-");
    // **広東語は繁体字の辞書で読める**（2026-09-01、ゲート2の指摘）。macOSの表示言語を
    // 粵語にすると `yue-Hant-HK` が来るが、`yue` の辞書は無いので**英語まで落ちていた**。
    // 香港・マカオの書き言葉は繁体字の中国語なので、`zh` として扱えば `zh-hant` に当たる
    // （話し言葉としては別の言語だが、**この辞書が担うのは書き言葉**）
    // **裸の `yue` も繁体字へ**（4巡目の指摘）。`zh` に置き換えるだけだと、
    // 地域が落ちたタグ（`navigator.languages` は地域を落とす。上の注記）で
    // **簡体字**に着く。CLDRの既定も `yue` → `yue-Hant-HK`
    if (parts[0] === "yue") {
      parts[0] = "zh";
      if (!parts.includes("hans") && !parts.includes("hant")) parts.splice(1, 0, "hant");
    }
    // **`es-MX` を `es-419` へ寄せる**（包含解決。dev #17）。BCP-47 では
    // `es-MX ⊂ es-419` だが、**この梯子は包含関係を知らない**——短くしていくだけなので
    // `es-mx` → `es` と、**中南米の人が本国の辞書に着く**。`419` を挟めば
    // `es-419` で当たる。
    // **地域が `es`（スペイン）なら寄せない**、が規則の全部である。地域が
    // 分からないタグ（裸の `es`）もそのまま——**どちらのスペイン語か分からないときは
    // 本国側**（辞書がそう書かれている）。
    // **赤道ギニアのように本国寄りの地域も中南米へ落ちる**が、
    // スペイン語の地域差は**相互に通じる**ので、そこは受け入れる（dev #17 の但し書き）。
    if (parts[0] === "es") {
      const region = regionOf(parts);
      // **見るのは「もう2番目に `419` が居るか」**（ゲート1）。`region !== "419"` で
      // 弾くと **`es-Latn-419`** が素通りする——地域は `419` なのに2番目は `latn` なので、
      // 梯子は `es-latn-419` → `es-latn` → `es` と**本国へ着く**
      if (region && region !== "es" && parts[1] !== "419") parts.splice(1, 0, "419");
    }
    // **書き言葉を省いたタグに補う**（`zh-TW` → `zh-hant-tw`）。台湾・香港・
    // マカオのOSは `zh-Hant` を省いて渡してくることがあり、そのままだと
    // `zh-hant` の辞書に当たらずに `zh`（簡体字）まで落ちてしまう
    if (parts[0] === "zh" && !parts.includes("hant") && isTraditionalChinese(parts))
      parts.splice(1, 0, "hant");
    for (let n = parts.length; n > 0; n--) {
      const code = parts.slice(0, n).join("-");
      // **繁体字を簡体字の辞書へ落とさない**（2026-09-01、両ゲートの指摘）。
      // **いまは `zh-hant.ts` があるので、ここまで来ない**——上の補完で `hant` が
      // 2番目に入り、`zh-hant` で当たって返る。残してあるのは
      // **繁体字の辞書が無くなったときの受け皿**で、そのときは英語へ落ちる。
      // 簡体字を出すよりましだから、ではない: 本文が簡体字・曜日が繁体字
      // （`formatLocale` は `zh-Hant-TW` のまま）・OSのダイアログが英語と、
      // **1画面が3つの書き言葉に割れる**
      if (code === "zh" && isTraditionalChinese(parts)) break;
      if (hasDict(code)) return code;
    }
  }
  return null;
}
