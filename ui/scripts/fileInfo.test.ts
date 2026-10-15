/**
 * ファイルの基本情報の書式（`dev/adr.file-info-and-context-menu.md` の A）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { extensionOf, formatSize, sizeBaseFor, splitPath } from "../src/fileInfo.ts";

test("the size is counted the way the OS file manager counts it", () => {
  assert.equal(sizeBaseFor("windows"), 1024);
  assert.equal(sizeBaseFor("macos"), 1000);
  assert.equal(sizeBaseFor("other"), 1000);
});

test("Windows: like Explorer, cut to three significant digits and move up past 1,000", () => {
  assert.equal(formatSize(Math.round(35.29 * 1024 ** 3), 1024, "en"), "35.2 GB");
  assert.equal(formatSize(Math.round(6.613 * 1024 ** 2), 1024, "en"), "6.61 MB");
  assert.equal(formatSize(1015 * 1024, 1024, "en"), "0.99 MB");
  assert.equal(formatSize(512 * 1024, 1024, "en"), "512 KB");
  assert.equal(formatSize(999, 1024, "en"), "999 B");
});

test("macOS: like Finder, divide by 1,000 and round to one decimal", () => {
  assert.equal(formatSize(9_244_000_000, 1000, "en"), "9.2 GB");
  assert.equal(formatSize(265_597, 1000, "en"), "265.6 KB");
  // 丸めると境目に届くものは次の単位へ
  assert.equal(formatSize(999_960, 1000, "en"), "1 MB");
  assert.equal(formatSize(0, 1000, "en"), "0 B");
});

test("a path splits into its folder and its name on either separator", () => {
  assert.deepEqual(splitPath("/Users/a/Pictures/IMG_1.JPG"), { folder: "/Users/a/Pictures", name: "IMG_1.JPG" });
  assert.deepEqual(splitPath("D:\\Photos\\2026\\a.cr3"), { folder: "D:\\Photos\\2026", name: "a.cr3" });
  assert.deepEqual(splitPath("C:\\a.jpg"), { folder: "C:\\", name: "a.jpg" });
  assert.deepEqual(splitPath("/a.jpg"), { folder: "/", name: "a.jpg" });
  assert.deepEqual(splitPath("a.jpg"), { folder: "", name: "a.jpg" });
});

test("the type is the extension in capitals", () => {
  assert.equal(extensionOf("IMG_1.cr3"), "CR3");
  assert.equal(extensionOf("photo.tar.jpg"), "JPG");
  assert.equal(extensionOf(".hidden"), "");
  assert.equal(extensionOf("noext"), "");
});
