/**
 * 取り込みの画面で最初から選ぶドライブ（pictkura-dev #39）の判定。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { branchTo, isViewing, openPathOf, preselectDrive, standsOut, type DriveLike } from "../src/importPreselect.ts";

const card = (path: string): DriveLike => ({ path, kind: "removable", dcim_path: path + "/DCIM" });
const removable = (path: string): DriveLike => ({ path, kind: "removable", dcim_path: null });
const fixed = (path: string): DriveLike => ({ path, kind: "fixed", dcim_path: null });
const network = (path: string): DriveLike => ({ path, kind: "network", dcim_path: null });

test("カードがちょうど1枚なら、それを選ぶ（ほかに取り外せるディスクがあっても）", () => {
  for (const platform of ["windows", "macos"] as const) {
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

test("固定ディスクは DCIM を持っていても選ばない（外付け HDD の控えを開いた瞬間に走査しない。ゲート2）", () => {
  const backup: DriveLike = { path: "X:\\", kind: "fixed", dcim_path: "X:\\DCIM" };
  assert.equal(preselectDrive([backup], "windows"), null);
  const sd = card("E:\\");
  assert.equal(preselectDrive([backup, sd], "windows"), sd, "カードが1枚あればそれ");
});

test("開くのは DCIM から（一番上から走査しない。ゲート2）", () => {
  assert.equal(openPathOf(card("/Volumes/SD")), "/Volumes/SD/DCIM");
  assert.equal(openPathOf(removable("E:\\")), "E:\\");
});

test("いま見ているフォルダがカードの中かは、区切りまで見る", () => {
  const sd = card("/Volumes/SD");
  assert.equal(isViewing("/Volumes/SD/DCIM/100TEST", sd), true);
  assert.equal(isViewing("/Volumes/SD", sd), true);
  assert.equal(isViewing("/Volumes/SD2/DCIM", sd), false, "名前の頭が同じ別のカード");
  assert.equal(isViewing(null, sd), false);
  const e = card("E:\\");
  assert.equal(isViewing("E:\\DCIM\\100CANON", e), true);
  assert.equal(isViewing("F:\\DCIM", e), false);
});

test("開いた場所の行が見えるまで、根からその親までの枝を開く（DCIM を開くとドライブが閉じたままだった）", () => {
  assert.deepEqual(branchTo(["C:\\", "J:\\"], "J:\\DCIM"), ["J:\\"]);
  assert.deepEqual(branchTo(["C:\\", "J:\\"], "J:\\DCIM\\100MSDCF"), ["J:\\", "J:\\DCIM"]);
  assert.deepEqual(branchTo(["/", "/Volumes/SD"], "/Volumes/SD/DCIM/100TEST"), ["/Volumes/SD", "/Volumes/SD/DCIM"]);
});

test("根そのものや、どの根の中でもない場所では何も開かない", () => {
  assert.deepEqual(branchTo(["C:\\", "J:\\"], "J:\\"), []);
  assert.deepEqual(branchTo(["/Volumes/SD"], "/Volumes/SD2/DCIM"), [], "名前の頭が同じ別のカード");
  assert.deepEqual(branchTo(["J:\\"], "K:\\DCIM"), []);
});

test("自分で足したフォルダは、それ自体が根なので上のドライブを開かない", () => {
  assert.deepEqual(branchTo(["/", "/Users/me/Pictures"], "/Users/me/Pictures"), []);
  assert.deepEqual(branchTo(["C:\\", "C:\\Photos"], "C:\\Photos"), []);
  // その中を開いたときは、近いほうの根（足したフォルダ）から辿る
  assert.deepEqual(branchTo(["C:\\", "C:\\Photos"], "C:\\Photos\\2026\\10"), ["C:\\Photos", "C:\\Photos\\2026"]);
});

test("macOS・Linux の名前に入った \\ は区切りではない", () => {
  assert.deepEqual(branchTo(["/", "/Volumes/CARD\\2026"], "/Volumes/CARD\\2026/DCIM/100X"), [
    "/Volumes/CARD\\2026",
    "/Volumes/CARD\\2026/DCIM",
  ]);
  assert.equal(isViewing("/Volumes/CARD\\2026/DCIM", card("/Volumes/CARD\\2026")), true);
  // Windows の UNC の根
  assert.deepEqual(branchTo(["\\\\nas\\photos"], "\\\\nas\\photos\\2026\\10"), ["\\\\nas\\photos", "\\\\nas\\photos\\2026"]);
});
