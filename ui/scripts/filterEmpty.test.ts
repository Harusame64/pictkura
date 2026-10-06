/**
 * 絞り込んで0件のときの見出しの条件（`src/filterEmpty.ts`、plan.filter-empty.md）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { cameraOnly, filterConditions, joinConditions } from "../src/filterEmpty.ts";
import { ja } from "../src/i18n/ja.ts";
import { en } from "../src/i18n/en.ts";
import type { Dict } from "../src/i18n/ja.ts";
import type { MediaFilter, MediaKind } from "../src/api.ts";

const labelsOf = (t: Dict) => ({
  shelf: { fav: t.navFavorites, picked: t.navPicked, outgoing: t.navOutgoing },
  kind: { photo: t.kindPhoto, raw: t.kindRaw, video: t.kindVideo },
  query: t.filterCondQuery,
  camera: t.filterCondCamera,
});

const titleOf = (t: Dict, filter: MediaFilter, kind: MediaKind, query: string) =>
  t.filterEmptyTitle(
    joinConditions(filterConditions(filter, kind, query, labelsOf(t)), t.listSeparator, t.andMore),
  );

test("左のカメラが入れた語だけのときに限って、機種名を取り出す", () => {
  assert.equal(cameraOnly('camera:"Canon EOS R5"'), "Canon EOS R5");
  assert.equal(cameraOnly('  camera:"X100V" '), "X100V");
  // ほかの語と混ざっている・閉じていない・空の名前は、カメラと見なさない
  assert.equal(cameraOnly('camera:"X100V" 海'), null);
  assert.equal(cameraOnly('camera:"X100V'), null);
  assert.equal(cameraOnly('camera:""'), null);
  assert.equal(cameraOnly("海"), null);
});

test("何も効いていなければ条件は空", () => {
  assert.deepEqual(filterConditions("all", "all", "  ", labelsOf(ja)), []);
});

test("条件は左の一覧の上から（棚 → 種類 → 検索語）並ぶ", () => {
  assert.deepEqual(filterConditions("fav", "video", "海", labelsOf(ja)), [
    "★ お気に入り",
    "動画",
    "検索語「海」",
  ]);
  assert.deepEqual(filterConditions("outgoing", "all", "", labelsOf(en)), ["Outgoing"]);
  assert.deepEqual(filterConditions("all", "raw", "", labelsOf(en)), ["RAW"]);
});

test("検索語がカメラだけなら機種名で言い、混ざっていれば打った語のまま言う", () => {
  assert.deepEqual(filterConditions("all", "all", 'camera:"X100V"', labelsOf(ja)), [
    "X100V で撮ったもの",
  ]);
  assert.deepEqual(filterConditions("picked", "all", 'camera:"X100V" 海', labelsOf(en)), [
    "⚑ Picked",
    "the search “camera:\"X100V\" 海”",
  ]);
});

test("見出しの最終形（ja / en）", () => {
  assert.equal(
    titleOf(ja, "fav", "video", "海"),
    "この絞り込みに該当する写真はありません：★ お気に入り、動画、検索語「海」",
  );
  assert.equal(
    titleOf(en, "fav", "video", "sea"),
    "No photos match these filters: ★ Favorites, Videos, the search “sea”",
  );
  assert.equal(titleOf(ja, "outgoing", "all", ""), "この絞り込みに該当する写真はありません：送り出し");
  assert.equal(titleOf(en, "all", "all", 'camera:"X100V"'), "No photos match these filters: taken with X100V");
});

test("つなぎ方は nameList と同じ: 3つまで出し、残りは数で言う", () => {
  const more = (n: number) => `+${n}`;
  assert.equal(joinConditions([], "、", more), "");
  assert.equal(joinConditions(["a"], "、", more), "a");
  assert.equal(joinConditions(["a", "b", "c"], ", ", more), "a, b, c");
  assert.equal(joinConditions(["a", "b", "c", "d", "e"], ", ", more), "a, b, c, +2");
});
