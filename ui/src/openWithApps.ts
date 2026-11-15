/**
 * 「他のアプリで開く」の候補の並べ方（2026-11-15 利用者「雑音が多いやつは、こちらでリストを持っておくと良いかも。
 * 有料・無料・有名なやつ」）。
 *
 * **名簿は探しに行くためのものではない。** 並べるのは OS が挙げたアプリだけで（`os_apps.rs`）、名簿はその中で
 * 上へ出すもの・区切りの下へ回すものを決める。名簿に無いアプリも、OS が勧めていれば上の段に残る。
 *
 * 鍵は Windows が exe のファイル名（小文字）、macOS がアプリの束 ID（小文字）。末尾の `*` は前方一致。
 * **会社名だけの前方一致はしない**（`jp.co.canon.*` はプリンタの道具まで上へ出す。/code-review）。
 * 編集ソフトの鍵は各社の名付けからの見込みで、実物で確かめたのは mac の雑音の束 ID（mdls）だけ
 */

/** OS が挙げたアプリ（Rust の `OsApp`） */
export type OsApp = {
  id: string;
  name: string;
  recommended: boolean;
  key: string;
};

/** よく知られた写真・動画のアプリ。上から順に並ぶ（有料 → カメラメーカー → 無料 → OS 付属） */
const KNOWN_PHOTO: readonly string[] = [
  // 有料
  "lightroom.exe", "com.adobe.lightroomclassic*", "com.adobe.lightroomcc*", "com.adobe.mas.lightroomcc*",
  "photoshop.exe", "com.adobe.photoshop*",
  "captureone.exe", "com.captureone.*", "com.phaseone.captureone*",
  "photo.exe", "affinity photo*.exe", "com.seriflabs.affinityphoto*",
  "dxo.photolab.exe", "com.dxo.photolab*",
  "luminar neo.exe", "com.skylum.luminar*",
  "on1 photo raw*.exe", "com.on1.photoraw*",
  "com.pixelmatorteam.pixelmator*", "com.flyingmeat.acorn*",
  // カメラメーカー
  "dpp4.exe", "jp.co.canon.dpp*", "nx studio.exe", "com.nikon.nxstudio*",
  "imaging edge*.exe", "com.sony.imagingedge*", "x raw studio.exe", "com.fujifilm.xrawstudio*", "om workspace.exe", "jp.co.omsystem.omworkspace*",
  // 無料
  "gimp*", "org.gimp.*", "darktable.exe", "org.darktable*", "rawtherapee*.exe", "com.rawtherapee*",
  "krita.exe", "org.kde.krita", "paintdotnet.exe", "i_view64.exe", "i_view32.exe", "xnviewmp.exe", "com.xnview.*",
  "fsviewer.exe", "digikam.exe", "org.kde.digikam",
  // OS 付属
  "mspaint.exe", "com.apple.preview", "com.apple.photos",
];

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
];

function matches(key: string, pattern: string): boolean {
  return pattern.endsWith("*") ? key.startsWith(pattern.slice(0, -1)) : key === pattern;
}

function rankOf(key: string, list: readonly string[]): number {
  const i = list.findIndex((p) => matches(key, p));
  return i < 0 ? Infinity : i;
}

/**
 * OS の候補を、上の段（名簿の順 → OS が勧める残り）と下の段（雑音）に分ける。
 * `video` は動画のとき（QuickTime や VLC は動画では上、写真では QuickTime は雑音）
 */
export function rankOsApps(apps: readonly OsApp[], video: boolean): { top: OsApp[]; rest: OsApp[] } {
  const known = video ? [...KNOWN_VIDEO, ...KNOWN_PHOTO] : KNOWN_PHOTO;
  const noise = video ? NOISE : [...NOISE, "com.apple.quicktimeplayerx", "wmplayer.exe"];
  const scored = apps.map((a, i) => ({ a, i, k: rankOf(a.key, known), n: rankOf(a.key, noise) < Infinity }));
  const top = scored
    // 上の段は OS が勧めたものだけ。名簿は並べ替えにだけ使う——名簿に載っていても、その種類を開けるとは
    // 限らない（Paint.NET は動画も RAW も開けないのに、mp4・CR3 で先頭に来た。win2 の実測）
    .filter((s) => !s.n && s.a.recommended)
    .sort((x, y) => x.k - y.k || x.i - y.i)
    .map((s) => s.a);
  const rest = scored.filter((s) => !top.includes(s.a)).map((s) => s.a);
  return { top, rest };
}
