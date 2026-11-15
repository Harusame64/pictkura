/**
 * 「他のアプリで開く」の候補の並べ方（`openWithApps.ts`）。`npm --prefix ui test`。
 * 顔ぶれは spike（dev/plan.md 2026-11-15）で OS が実際に返したもの。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { rankOsApps, type OsApp } from "../src/openWithApps.ts";

const app = (key: string, name: string, recommended = true): OsApp => ({ id: key, name, recommended, key });

test("Windows, a JPEG: known editors first, OS suggestions next, noise below", () => {
  const apps = [
    app("snapswift", "SnapSwift"),
    app("フォト", "フォト"),
    app("snippingtool.exe", "Snipping Tool"),
    app("mspaint.exe", "ペイント"),
    app("paintdotnet.exe", "Paint.NET"),
    app("notepad.exe", "メモ帳", false),
    app("winword.exe", "Word", false),
    app("gimp-2.10.exe", "GIMP", false),
  ];
  const { top, rest } = rankOsApps(apps, false);
  // 勧められたものを、名簿の順（Paint.NET → ペイント）、そのあと名簿外（OS の順）
  assert.deepEqual(top.map((a) => a.name), ["Paint.NET", "ペイント", "SnapSwift", "フォト"]);
  // 勧めていても雑音は下（Snipping Tool）。**名簿に載っていても、勧められていなければ下**（GIMP）
  assert.deepEqual(rest.map((a) => a.name), ["Snipping Tool", "メモ帳", "Word", "GIMP"]);
});

test("macOS, a JPEG: QuickTime and browsers go below, Parallels apps too", () => {
  const apps = [
    app("com.apple.preview", "プレビュー"),
    app("com.apple.quicktimeplayerx", "QuickTime Player"),
    app("com.google.chrome", "Google Chrome"),
    app("com.anthropic.claudefordesktop", "Claude"),
    app("com.parallels.winapp.abc", "ペイント", false),
    app("com.microsoft.edgemac", "Microsoft Edge"),
    // 実物の束 ID（mdls で確かめた。名前からの推測「image-playground」は外れていた）
    app("com.apple.generativeplaygroundapp", "Image Playground"),
  ];
  const { top, rest } = rankOsApps(apps, false);
  assert.deepEqual(top.map((a) => a.name), ["プレビュー"]);
  assert.deepEqual(rest.map((a) => a.name), [
    "QuickTime Player",
    "Google Chrome",
    "Claude",
    "ペイント",
    "Microsoft Edge",
    "Image Playground",
  ]);
});

test("a video puts players and video editors up, QuickTime included", () => {
  const apps = [app("com.apple.preview", "プレビュー"), app("com.apple.quicktimeplayerx", "QuickTime Player"), app("org.videolan.vlc", "VLC")];
  const { top } = rankOsApps(apps, true);
  assert.deepEqual(top.map((a) => a.name), ["VLC", "QuickTime Player", "プレビュー"]);
});

test("nothing is dropped: every app the OS offered is in one of the two lists", () => {
  const apps = [app("a.exe", "A", false), app("photoshop.exe", "Photoshop"), app("chrome.exe", "Chrome")];
  const { top, rest } = rankOsApps(apps, false);
  assert.equal(top.length + rest.length, apps.length);
  assert.deepEqual(top.map((a) => a.name), ["Photoshop"]);
});

test("a known editor the OS does not recommend for this type stays below (win2: Paint.NET for mp4 and CR3)", () => {
  const apps = [
    app("paintdotnet.exe", "Paint.NET", false),
    app("フォト", "フォト"),
    app("clipchamp", "Microsoft Clipchamp"),
  ];
  const { top, rest } = rankOsApps(apps, true);
  assert.deepEqual(top.map((a) => a.name), ["フォト", "Microsoft Clipchamp"]);
  assert.deepEqual(rest.map((a) => a.name), ["Paint.NET"]);
});

test("a * inside a key matches too (codex: rawtherapee*.exe never matched)", () => {
  const apps = [
    app("snapswift", "SnapSwift"),
    app("rawtherapee-5.11.exe", "RawTherapee"),
    app("affinity photo 2.exe", "Affinity Photo 2"),
    app("rawtherapee.txt", "not an exe"),
  ];
  const { top } = rankOsApps(apps, false);
  // 名簿に載ったものが名簿の順で先、名簿外（SnapSwift・拡張子違い）は OS の順で後
  assert.deepEqual(top.map((a) => a.name), ["Affinity Photo 2", "RawTherapee", "SnapSwift", "not an exe"]);
});

test("a RAW keeps editors found under other types only when they open RAW (user: 'RAW 用もあるの？')", () => {
  const apps: OsApp[] = [
    app("フォト", "フォト"),
    app("notepad.exe", "メモ帳", false),
    // DNG・TIFF・JPEG の一覧から拾ったもの（via 付き）。OS は RAW の拡張子には挙げていない
    { ...app("lightroom.exe", "Adobe Lightroom Classic", false), via: "dng" },
    { ...app("silkypix developer studio pro11.exe", "SILKYPIX", false), via: "tif" },
    { ...app("paintdotnet.exe", "Paint.NET", false), via: "jpg" },
    { ...app("winword.exe", "Word", false), via: "jpg" },
  ];
  const { top, rest } = rankOsApps(apps, false, true);
  // 名簿の順（Lightroom → SILKYPIX）、そのあと名簿外の推奨（フォト）。Paint.NET・Word は RAW に出さない
  assert.deepEqual(top.map((a) => a.name), ["Adobe Lightroom Classic", "SILKYPIX", "フォト"]);
  assert.deepEqual(rest.map((a) => a.name), ["メモ帳"]);
});

test("on a RAW, a RAW editor goes on top whether the OS listed it for the RAW or for another type", () => {
  // RAW の一覧に推奨なしで居る Lightroom も上（別の形式から拾ったときと同じ扱い。/code-review）
  const own = rankOsApps([app("フォト", "フォト"), app("lightroom.exe", "Lightroom", false)], false, true);
  assert.deepEqual(own.top.map((a) => a.name), ["Lightroom", "フォト"]);
  // RAW でなければ、勧められていない Lightroom は下のまま
  const jpeg = rankOsApps([app("フォト", "フォト"), app("lightroom.exe", "Lightroom", false)], false, false);
  assert.deepEqual(jpeg.top.map((a) => a.name), ["フォト"]);
});

test("Parallels' Windows apps never go up, even when their name matches the list", () => {
  const { top } = rankOsApps(
    [app("com.apple.preview", "プレビュー"), { ...app("com.parallels.winapp.silkypix", "SILKYPIX", false), via: "jpg" }],
    false,
    true,
  );
  assert.deepEqual(top.map((a) => a.name), ["プレビュー"]);
});

test("a remembered app stays on the shortcut unless the list knows it cannot open the file", async () => {
  const { suitsFile } = await import("../src/openWithApps.ts");
  assert.equal(suitsFile("paintdotnet.exe", false, false), true);
  assert.equal(suitsFile("paintdotnet.exe", true, false), false, "Paint.NET for a RAW");
  assert.equal(suitsFile("paintdotnet.exe", false, true), false, "Paint.NET for a video");
  assert.equal(suitsFile("lightroom.exe", true, false), true);
  assert.equal(suitsFile("vlc.exe", false, true), true);
  assert.equal(suitsFile("vlc.exe", false, false), false, "VLC for a photo");
  // 名簿に無いアプリは利用者の選択なので、どれにも残す
  assert.equal(suitsFile("myeditor.exe", true, false), true);
  assert.equal(suitsFile("myeditor.exe", false, true), true);
});
