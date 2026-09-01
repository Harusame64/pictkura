/**
 * UI文字列の多言語対応。
 *
 * 方針:
 * - ランタイムを増やさない（i18nライブラリを入れない）。辞書は素のオブジェクトで、
 *   バンドルに乗るのは選ばれた言語ではなく全言語だが、UI文字列は数KBなので
 *   起動時間には響かない（＝分割ロードの複雑さを買う理由がない）。
 * - **辞書は言語ごとに1ファイル**（`ja.ts` / `en.ts` …）。言語の追加は
 *   **ファイルを1つ足し、このファイルの `DICTS` と `LOCALES` に1行ずつ**。
 *   キーの型は `ja.ts` から導出しているので、抜けても余ってもコンパイルエラーになる。
 *   **`LOCALES` への追加を忘れると、OSの言語がそれだったときは出るのに
 *   設定からは選べない**という半端な状態になる。
 * - **取扱説明書は前提条件ではない**。`Settings.tsx` の `manualDoc` は
 *   ja 以外なら英語版を先に探すので、`docs/manual.<言語>.html` が無くても
 *   説明書ボタンは英語版を開く（404にも死んだボタンにもならない）。
 *   置けば配布物にはグロブで入る（CIが「リポジトリにある分が全部入っているか」を見る）。
 * - 日付・数値の書式は辞書に持たず `Intl` に任せる（全ロケールが無料で正しくなる）。
 * - **言葉のコードと書式のコードは別**（`locale` と `formatLocale`）。辞書は
 *   地域を落として選ぶが（`es-MX` → `es`）、**その丸めを書式に持ち込まない**。
 *   持ち込むとメキシコの人に本国スペインの桁区切りと月曜始まりが出る。
 *   地域つきのタグは **Rustが起動時に渡す**（WebViewは地域を落とす）。
 * - 話者数の多い言語を優先。RTL（アラビア語等）はレイアウトの論理プロパティ化が
 *   済んでから追加する。
 */

import { ja, type Dict } from "./ja";
import { en } from "./en";
import { de } from "./de";
import { es } from "./es";
import { zh } from "./zh";

export type { Dict };

/** 対応言語。**ここと `LOCALES` の両方**に足すこと（片方だけだと半端になる） */
const DICTS: Record<string, Dict> = { ja, en, de, es, zh };

/**
 * 選択肢に出す言語（コードと、その言語自身での呼び名）。
 *
 * **呼び名はその言語で書く**。英語しか読めない画面になってしまった人が
 * 日本語へ戻れるように、「日本語」は日本語のまま出す（"Japanese" にしない）。
 */
export const LOCALES: { code: string; label: string }[] = [
  { code: "ja", label: "日本語" },
  { code: "en", label: "English" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
  { code: "zh", label: "简体中文" },
];

/** 言語の指定を置く場所（テーマと同じくlocalStorage） */
const LOCALE_KEY = "pictkura.locale";

/**
 * **OSが持っている言語タグ（地域つき）。Rustが起動時にここへ置く**
 * （`src-tauri/src/lib.rs` の `os_locale_plugin`）。
 *
 * **WebViewの `navigator.languages` は地域を落とす。** 同じ画面・同じ瞬間の実測
 * （2026-09-01、macOS）:
 *
 * ```
 * __PICTKURA_OS_LOCALES__ = ["ja-JP"]   ← 地域あり
 * navigator.languages     = ["ja"]      ← 裸
 * Intl の既定             = ja          ← 裸
 * ```
 *
 * Windowsでは `Intl` の既定に地域が乗ることがあるが、**表示言語と地域が食い違うと
 * そこも裸に落ちる**（`Set-Culture es-MX` で確認）。**地域が要るのはまさにその人たち**
 * なので、噛み合っているときだけ効く経路は当てにしない。
 *
 * **無いことがある**——開発中に `vite` の画面をブラウザで開いたとき、
 * Rustが値を置く前の版で動かしたとき。**そのときは `navigator` に落ちる**（従来動作）。
 */
const osLocales: string[] = (() => {
  const raw = (window as unknown as { __PICTKURA_OS_LOCALES__?: unknown })
    .__PICTKURA_OS_LOCALES__;
  return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : [];
})();

/**
 * **地域書式の設定**（数字の桁区切り・日付の並び・週の始まり）。Rustが置く。
 *
 * **上の言語リストとは別の設定**。表示言語と地域を食い違わせている人
 * ——Windowsで表示は日本語のまま `Set-Culture es-MX` にした場合など——では
 * 両者が別の値になり、**書式に要るのはこちら**。
 * 取れないOS（Linux）や古い版では `null`。そのときは言語リストへ倒す。
 */
const osRegion: string | null = (() => {
  const raw = (window as unknown as { __PICTKURA_OS_REGION__?: unknown })
    .__PICTKURA_OS_REGION__;
  return typeof raw === "string" && raw.length > 0 ? raw : null;
})();

/**
 * 照合に使う優先リスト。**Rustが渡したものを先に見る**（地域が落ちていない）。
 * 後ろに `navigator` を繋いでおくのは、Rustが無い場面でも今までどおり動かすため。
 */
const preferredLocales: string[] = [
  ...osLocales,
  ...(navigator.languages ?? [navigator.language]),
];

/**
 * その言語コードの辞書を持っているか。
 *
 * **`DICTS[code]` で判定してはいけない**。`DICTS` は素のオブジェクトなので
 * `"constructor"` や `"toString"` が真になり、辞書のつもりで**関数**を掴む。
 * そうなると画面じゅうが `undefined` になる（localStorageを直に書き換えた場合に届く）。
 *
 * `Object.hasOwn` はES2022なので、この設定（ES2021）では使えない。
 */
const hasDict = (code: string) =>
  Object.prototype.hasOwnProperty.call(DICTS, code);

/**
 * localStorageの読み書き。**失敗しても落とさない**。
 *
 * `locale` はモジュールを読んだ時点で決まるので、ここで例外が飛ぶと
 * i18nを読み込む画面すべてが真っ白になる。言語の指定は無くても既定で動く類の
 * 情報なので、読めない・書けないときは黙って諦める。
 */
function readStored(): string | null {
  try {
    return localStorage.getItem(LOCALE_KEY);
  } catch {
    return null;
  }
}
function writeStored(code: string | null): boolean {
  try {
    if (code === null) localStorage.removeItem(LOCALE_KEY);
    else localStorage.setItem(LOCALE_KEY, code);
    return true;
  } catch {
    return false;
  }
}

/**
 * 表示に使う言語コードを決める。
 *
 * **設定で選んだ言語 → OSの優先言語 → 英語** の順。設定を先に見るのは、
 * OSが日本語でも英語で使いたい人がいるため（説明書のスクリーンショットを
 * 撮るときにも要る）。
 */
function pickLocale(): string {
  const chosen = readStored();
  if (chosen && hasDict(chosen)) return chosen;
  for (const tag of preferredLocales) {
    // "ja-JP" → "ja" のように地域を落として照合する
    const base = tag.toLowerCase().split("-")[0];
    if (DICTS[tag.toLowerCase()]) return tag.toLowerCase();
    if (DICTS[base]) return base;
  }
  return "en";
}

/** 現在の言語コード（"ja" / "en" …） */
export const locale = pickLocale();

/** 設定で選ばれている言語（未指定なら `null` ＝ OSに合わせる） */
export function readLocaleChoice(): string | null {
  const chosen = readStored();
  return chosen && hasDict(chosen) ? chosen : null;
}

/**
 * 言語を切り替える。`null` を渡すとOSの優先言語に戻る。
 *
 * **画面を読み込み直す**のが要点。辞書 `t` はモジュールを読んだ時点で
 * 決まる定数で、画面のあちこちが直接それを読んでいる。差し替えて回るより、
 * 読み直したほうが取りこぼしが無い（ローカルのWebViewなので一瞬で、
 * 表示中の内容はバックエンドから引き直される）。
 *
 * **切り替わったかを返す**。保存できなければ言語は変わらないので、
 * 呼び出し側が「その言語になっている」と表示してしまわないようにする。
 * 保存が効かない状態でメモリ上だけ切り替えても、読み込み直した先で
 * 元に戻るだけで、かえって分からなくなる。
 */
export function setLocaleChoice(code: string | null): boolean {
  if (code !== null && !hasDict(code)) return false;
  if (!writeStored(code)) return false;
  // **書いたあとで決め直す**。「OSに合わせる」を選んだ結果いまと同じ言語に
  // なることもあるので、指定そのものではなく**結果**を見て判断する
  if (pickLocale() !== locale) location.reload();
  return true;
}

/** 現在の辞書。`t.searchPlaceholder` のように使う */
export const t: Dict = DICTS[locale] ?? en;

/**
 * **書式に使うロケール。辞書のコード（`locale`）とは別物。**
 *
 * `locale` は**辞書を選ぶために地域を落とした**コード（`es-MX` → `es`）。
 * これを `Intl` にも渡していたので、**辞書の都合の丸めが書式まで巻き込んでいた**
 * ——メキシコの人に本国スペインの桁区切り（`12.345,6`）と月曜始まりが出る。
 * イギリスの人も `en` に落ちて**日曜始まり（米国式）**になっていた。
 *
 * 分けたので、**言葉は一番近い辞書・数字と日付は自分の地域**になる。
 * これはWindows / macOS の流儀と同じ（表示言語と地域は別の設定）。
 * 日本語のPCで `Español` を選ぶと、言葉はスペイン語・日付は日本のままになる。
 * **辞書の選択に連動させないこと**——連動させると、言語を選び直した瞬間に
 * 日付の書き方まで変わって、地域の設定が無視される。
 *
 * `locale` へ落ちるのは、OSが何も渡してこなかったとき（開発中のブラウザなど）。
 * そのときは今までどおりの挙動になる。
 */
export const formatLocale: string = (() => {
  // **OSのタグをそのまま使ってはいけない。** `Intl` は言語副タグを見て
  // **月名・曜日名・数字の字形・暦**まで決める。タイ語のOSで辞書が英語に落ちた人に
  // `th-TH` を渡すと、英語の画面に `สิงหาคม 2569`（タイ文字＋仏暦）が出る。
  // アラビア語のOSなら `١٢٬٣٤٥`。**言葉は辞書、地域だけOS**が正しい組み合わせ。
  for (const tag of [osRegion, ...preferredLocales]) {
    if (!tag) continue;
    try {
      const region = new Intl.Locale(tag).region;
      if (!region) continue;
      // `Intl.Locale` は綴りが不正だと `RangeError` を投げる。ここで例外が飛ぶと
      // i18nを読む画面が全部真っ白になるので、投げた候補は飛ばして次を見る
      return new Intl.Locale(locale, { region }).toString();
    } catch {
      // 次の候補へ
    }
  }
  return locale;
})();

/** 日付・時刻の書式はIntlに任せる（全ロケールが自動的に正しくなる） */
export const formatDateTime = (ms: number) =>
  new Date(ms).toLocaleString(formatLocale);

/** day_key（YYYYMMDD整数）を、その言語の日付表記にする */
export const formatDayKey = (dayKey: number) => {
  const y = Math.floor(dayKey / 10000);
  const m = Math.floor(dayKey / 100) % 100;
  const d = dayKey % 100;
  return new Date(y, m - 1, d).toLocaleDateString(formatLocale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
};

/** 「2026年8月」等の月見出し */
export const formatMonth = (year: number, month: number) =>
  new Date(year, month - 1, 1).toLocaleDateString(formatLocale, {
    year: "numeric",
    month: "long",
  });

/**
 * 週の始まり（`Date.getDay()` と同じ 0=日曜 … 6=土曜）。
 *
 * **辞書に持たない**——曜日名と同じく `Intl` から取る（辞書の冒頭の方針どおり）。
 * 日曜始まりは日本・北米などの習慣で、**ヨーロッパのほとんどとISO 8601は月曜始まり**。
 * ここを固定したままドイツ語だけ足すと、訳は正しいのに**カレンダーが外国のもの**に見える。
 *
 * `getWeekInfo()` が新しい綴りで、`weekInfo` が古い綴り。**両方見る**——
 * WebViewの実体はWindowsがWebView2、macOSがWKWebViewで、後者はOSの版に縛られる。
 *
 * **どちらも無い環境では、地域を見ていても週の始まりは直らない**（ゲート2の指摘）。
 * 下の最後の行は言語しか見ていないので、`en-GB` は `en` で始まるぶん日曜始まりに倒れる
 * ——Safari 17 より前の WKWebView がこれに当たる。**それでも表を手書きしない**：
 * どの地域が日曜始まりかはCLDRのデータで、辞書に持ち込むと**`Intl` に任せるという
 * この file の方針を崩す**うえ、手で写した時点で古びる。ここは
 * **いま出している挙動を変えない**側へ倒したまま置く（日本語と英語は日曜始まり）。
 * 直すなら `Intl` が使える環境かどうかではなく、**データをどこから持つか**の判断が先。
 */
export const firstWeekday: number = (() => {
  try {
    const l = new Intl.Locale(formatLocale) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    // ISO の 1=月曜 … 7=日曜。`% 7` で 0=日曜 … 6=土曜へ移す
    const firstDay = (l.getWeekInfo?.() ?? l.weekInfo)?.firstDay;
    if (typeof firstDay === "number") return firstDay % 7;
  } catch {
    // Intl.Locale が無い・ロケール名を受け付けない。下のフォールバックへ
  }
  return formatLocale.startsWith("ja") || formatLocale.startsWith("en") ? 0 : 1;
})();

/**
 * 曜日の見出し7つ。**週の始まりから並べる**ので、先頭が日曜とは限らない。
 *
 * 基準日には**日曜だと分かっている実在の日付**（2024-01-07）を使い、
 * 書式化も `UTC` で固定する。ローカル時刻のままだと、負のオフセットの地域で
 * **1日ずれた曜日名が出る**。
 */
export const weekdayLabels: string[] = (() => {
  const fmt = new Intl.DateTimeFormat(formatLocale, {
    weekday: "short",
    timeZone: "UTC",
  });
  // 2024-01-07 は日曜。そこから firstWeekday ぶんずらして7日ぶん
  return Array.from({ length: 7 }, (_, i) =>
    fmt.format(new Date(Date.UTC(2024, 0, 7 + ((firstWeekday + i) % 7)))),
  );
})();

/**
 * 動画の長さ（ミリ秒）を `0:12` / `1:02:03` にする（第9部）。
 *
 * Intlの `DurationFormat` は「1時間2分3秒」のように綴るので一覧のバッジには
 * 長すぎる。時計表記はどの言語でも同じ形なので、辞書にも入れない。
 */
export const formatDuration = (ms: number) => {
  const total = Math.max(0, Math.round(ms / 1000));
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
};

/** 件数などの数値（桁区切りをロケールに合わせる） */
export const formatNumber = (n: number) => n.toLocaleString(formatLocale);
