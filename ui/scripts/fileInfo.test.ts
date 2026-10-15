/**
 * ファイルの基本情報の書式（`dev/adr.file-info-and-context-menu.md` の A）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { extensionOf, formatBytes, sizeBaseFor, splitPath } from "../src/fileInfo.ts";

test("the size is counted the way the OS file manager counts it", () => {
  assert.equal(sizeBaseFor("windows"), 1024);
  assert.equal(sizeBaseFor("macos"), 1000);
  assert.equal(sizeBaseFor("other"), 1000);
  // 利用者の 4K 動画（エクスプローラーで 8.61 GB）
  const bytes = Math.round(8.61 * 1024 ** 3);
  assert.equal(formatBytes(bytes, 1024, "en"), "8.61 GB");
  assert.equal(formatBytes(bytes, 1000, "en"), "9.24 GB");
});

test("small and large sizes keep about three significant digits", () => {
  assert.equal(formatBytes(0, 1000, "en"), "0 byte");
  assert.equal(formatBytes(999, 1000, "en"), "999 byte");
  assert.equal(formatBytes(1000, 1000, "en"), "1 kB");
  assert.equal(formatBytes(12_345_678, 1000, "en"), "12.3 MB");
  assert.equal(formatBytes(123_456_789, 1000, "en"), "123 MB");
  assert.equal(formatBytes(5 * 1000 ** 5, 1000, "en"), "5,000 TB");
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
