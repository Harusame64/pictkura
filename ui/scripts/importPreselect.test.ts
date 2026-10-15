/**
 * 取り込みの画面で最初から選ぶドライブ（pictkura-dev #39）の判定。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { preselectDrive, standsOut, type DriveLike } from "../src/importPreselect.ts";

const card = (path: string): DriveLike => ({ path, kind: "removable", dcim_path: path + "/DCIM" });
const removable = (path: string): DriveLike => ({ path, kind: "removable", dcim_path: null });
const fixed = (path: string): DriveLike => ({ path, kind: "fixed", dcim_path: null });
const network = (path: string): DriveLike => ({ path, kind: "network", dcim_path: null });

test("カードがちょうど1枚なら、それを選ぶ（ほかに取り外せるディスクがあっても）", () => {
  for (const platform of ["windows", "macos"]) {
    const sd = card("/Volumes/SD");
    assert.equal(preselectDrive([fixed("/"), removable("/Volumes/Backup"), sd], platform), sd, platform);
  }
});

test("カードが2枚以上なら選ばない（目立たせるだけ）", () => {
  assert.equal(preselectDrive([card("E:\\"), card("F:\\")], "windows"), null);
});

test("macOS では、カードが無ければ取り外せるディスクでも選ばない（バックアップ用ディスクを丸ごと走査しない）", () => {
  assert.equal(preselectDrive([fixed("/"), removable("/Volumes/Backup")], "macos"), null);
});

test("Windows では、カードが無くても取り外せるドライブが1つなら選ぶ（USB メモリ）", () => {
  const usb = removable("E:\\");
  assert.equal(preselectDrive([fixed("C:\\"), network("Z:\\"), usb], "windows"), usb);
  assert.equal(preselectDrive([fixed("C:\\"), usb, removable("F:\\")], "windows"), null, "2つなら選ばない");
});

test("取り外せるものが何も無ければ選ばない", () => {
  assert.equal(preselectDrive([fixed("C:\\"), network("Z:\\")], "windows"), null);
  assert.equal(preselectDrive([], "macos"), null);
});

test("目立たせるのはカードと取り外せるドライブ", () => {
  assert.equal(standsOut(card("E:\\")), true);
  assert.equal(standsOut(removable("/Volumes/Backup")), true);
  assert.equal(standsOut(fixed("C:\\")), false);
  assert.equal(standsOut(network("Z:\\")), false);
});
