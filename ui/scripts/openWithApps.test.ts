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
  // 名簿の順（無料の GIMP・Paint.NET → OS 付属のペイント）、そのあと OS が勧める名簿外（OS の順）
  assert.deepEqual(top.map((a) => a.name), ["GIMP", "Paint.NET", "ペイント", "SnapSwift", "フォト"]);
  // 勧めていても雑音は下（Snipping Tool）。勧められていない名簿外も下
  assert.deepEqual(rest.map((a) => a.name), ["Snipping Tool", "メモ帳", "Word"]);
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
