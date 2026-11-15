/**
 * 「他のアプリで開く」の候補の並べ方（2026-11-15 利用者「雑音が多いやつは、こちらでリストを持っておくと良いかも。
 * 有料・無料・有名なやつ」）。
 *
 * **名簿は探しに行くためのものではない。** 並べるのは OS が挙げたアプリだけで（`os_apps.rs`）、名簿はその中で
 * 上へ出すもの・区切りの下へ回すものを決める。名簿に無いアプリも、OS が勧めていれば上の段に残る。
 *
 * 鍵は Windows が exe のファイル名（小文字）、macOS がアプリの束 ID（小文字）。`*` は何文字でも（途中にも書ける）。
 * **会社名だけの前方一致はしない**（`jp.co.canon.*` はプリンタの道具まで上へ出す。/code-review）。
 * 編集ソフトの鍵は各社の名付けからの見込みで、実物で確かめたのは mac の雑音の束 ID（mdls）だけ
 */

/** OS が挙げたアプリ（Rust の `OsApp`） */
export type OsApp = {
  id: string;
  name: string;
  recommended: boolean;
  key: string;
  /** RAW のとき、別の形式（dng・tif・jpg）に OS が挙げたアプリ。名簿で RAW を扱えるとしたものだけ残す */
  via?: string | null;
};

/**
 * よく知られた写真のアプリ。上から順に並ぶ（有料 → カメラメーカー → 無料 → OS 付属）。`raw` は RAW を開けるもの
 * （現像ソフト・カメラメーカー純正・RAW の閲覧ソフト）。**1 本の表に印で持つ**——別の表にすると片方だけ足して
 * 黙ってずれる（/code-review）
 */
const R = true;
const KNOWN_PHOTO_TABLE: readonly (readonly [string, boolean?])[] = [
  // 有料
  ["lightroom*.exe", R], ["com.adobe.lightroom*", R], ["com.adobe.mas.lightroomcc*", R],
  ["photoshop*.exe", R], ["com.adobe.photoshop*", R], ["adobe bridge*.exe", R], ["com.adobe.bridge*", R],
  ["captureone*.exe", R], ["com.captureone.*", R], ["com.phaseone.captureone*", R],
  ["photo.exe", R], ["affinity photo*.exe", R], ["com.seriflabs.affinityphoto*", R],
  ["dxo.photolab.exe", R], ["com.dxo.photolab*", R],
  ["luminar*.exe", R], ["com.skylum.luminar*", R],
  ["on1 photo raw*.exe", R], ["com.on1.photoraw*", R],
  ["com.pixelmatorteam.pixelmator*", R], ["com.flyingmeat.acorn*"],
  ["acdsee*.exe", R], ["com.acdsystems.acdsee*", R],
  ["photo mechanic*.exe", R], ["photomechanic*.exe", R], ["com.camerabits.photomechanic*", R],
  ["*silkypix*", R], ["*fastrawviewer*", R],
  // カメラメーカー
  ["dpp4.exe", R], ["jp.co.canon.dpp*", R], ["nx studio.exe", R], ["com.nikon.nxstudio*", R],
  ["imaging edge*.exe", R], ["com.sony.imagingedge*", R], ["x raw studio.exe", R], ["com.fujifilm.xrawstudio*", R],
  ["om workspace.exe", R], ["jp.co.omsystem.omworkspace*", R], ["olympus workspace.exe", R],
  ["*digital camera utility*", R], ["*pentax*utility*", R],
  // 無料
  ["gimp*"], ["org.gimp.*"], ["darktable.exe", R], ["org.darktable*", R], ["rawtherapee*.exe", R],
  ["com.rawtherapee*", R], ["krita.exe"], ["org.kde.krita"], ["paintdotnet.exe"], ["i_view64.exe"], ["i_view32.exe"],
  ["xnviewmp.exe", R], ["com.xnview.*", R], ["fsviewer.exe"], ["digikam.exe", R], ["org.kde.digikam", R],
  // OS 付属
  ["mspaint.exe"], ["com.apple.preview"], ["com.apple.photos"],
];
const KNOWN_PHOTO: readonly string[] = KNOWN_PHOTO_TABLE.map(([k]) => k);
const RAW_CAPABLE: readonly string[] = KNOWN_PHOTO_TABLE.filter(([, raw]) => raw).map(([k]) => k);

const KNOWN_VIDEO: readonly string[] = [
  "vlc.exe", "org.videolan.vlc",
  "resolve.exe", "com.blackmagic-design.davinciresolve*",
  "adobe premiere pro.exe", "com.adobe.premierepro*",
  "com.apple.finalcutpro*", "com.apple.imovieapp",
  "com.apple.quicktimeplayerx",
];

/** 写真には向かない（開けるだけの）アプリ。区切りの下へ */
const NOISE: readonly string[] = [
  "notepad.exe", "winword.exe", "excel.exe", "powerpnt.exe", "acrobat.exe", "acrord32.exe", "code.exe",
  "msedge.exe", "chrome.exe", "firefox.exe", "snippingtool.exe",
  "com.google.chrome*", "com.microsoft.edgemac*", "com.apple.safari", "org.mozilla.firefox",
  "com.anthropic.*", "com.apple.colorsyncutility", "com.apple.generativeplaygroundapp", "com.microsoft.vscode",
  // 仮想機械（Parallels）が Mac に出す Windows のアプリ。名簿の名前に当たっても上げない
  "com.parallels.winapp.*",
];

/** `*` はどこに書いても「何文字でも」（`rawtherapee*.exe`・`gimp*`）。ほかの字はそのまま比べる */
function matches(key: string, pattern: string): boolean {
  const re = new RegExp(`^${pattern.split("*").map((p) => p.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`);
  return re.test(key);
}

function rankOf(key: string, list: readonly string[]): number {
  const i = list.findIndex((p) => matches(key, p));
  return i < 0 ? Infinity : i;
}

/**
 * OS の候補を、上の段（名簿の順 → OS が勧める残り）と下の段（雑音）に分ける。
 * `video` は動画のとき（QuickTime や VLC は動画では上、写真では QuickTime は雑音）
 */
export function rankOsApps(
  apps: readonly OsApp[],
  video: boolean,
  raw = false,
): { top: OsApp[]; rest: OsApp[] } {
  const known = video ? [...KNOWN_VIDEO, ...KNOWN_PHOTO] : KNOWN_PHOTO;
  const noise = video ? NOISE : [...NOISE, "com.apple.quicktimeplayerx", "wmplayer.exe"];
  const rawCapable = (a: OsApp) => rankOf(a.key, RAW_CAPABLE) < Infinity;
  // 別の形式から拾ったもの（via）は、RAW を開けると名簿にあるものだけ。**RAW では、RAW を開ける名簿のアプリは
  // どこから来ても上の段**——RAW の一覧に推奨なしで居るか、別の形式から拾ったかで上下が分かれていた（/code-review）
  const own = apps.filter((a) => !a.via || rawCapable(a));
  const scored = own.map((a, i) => ({
    a: raw && rawCapable(a) ? { ...a, recommended: true } : a,
    i,
    k: rankOf(a.key, known),
    n: rankOf(a.key, noise) < Infinity,
  }));
  const top = scored
    // 上の段は OS が勧めたものだけ。名簿は並べ替えにだけ使う——名簿に載っていても、その種類を開けるとは
    // 限らない（Paint.NET は動画も RAW も開けないのに、mp4・CR3 で先頭に来た。win2 の実測）
    .filter((s) => !s.n && s.a.recommended)
    .sort((x, y) => x.k - y.k || x.i - y.i)
    .map((s) => s.a);
  const rest = scored.filter((s) => !top.includes(s.a)).map((s) => s.a);
  return { top, rest };
}

/**
 * 覚えたアプリ（利用者が選んだもの）を、この写真の Ctrl+1〜3 に載せてよいか。**名簿が「開けない」と知っている
 * ときだけ外す**——写真用と分かっているアプリを RAW・動画に（Paint.NET で mp4 等）、動画用を写真に。名簿に無い
 * アプリは利用者の選択なので外さない。`key` は exe のファイル名か束 ID（小文字）
 */
export function suitsFile(key: string, raw: boolean, video: boolean): boolean {
  const photo = rankOf(key, KNOWN_PHOTO) < Infinity;
  const videoApp = rankOf(key, KNOWN_VIDEO) < Infinity;
  if (video) return !photo || videoApp;
  if (raw) return !photo || rankOf(key, RAW_CAPABLE) < Infinity;
  return !videoApp || photo;
}
