/**
 * 絞り込んで0件のときの見出しに並べる「効いている条件」（pictkura-dev `plan.filter-empty.md`、
 * 2026-10-07 利用者決定: 案B）。純関数。`scripts/filterEmpty.test.ts` が表で見る。
 *
 * 並べる順は**左の一覧の上から**——棚（★・⚑・送り出し）、種類（画像・RAW・動画）、検索語。
 * カメラは専用の状態を持たず、左のカメラを押すと検索語 `camera:"X"` が入るだけなので、
 * **検索語がそれだけのとき**に限って「X で撮ったもの」と言い換える。ほかの語と混ざっていれば
 * 打った語のまま「検索語「…」」と言う（言い換えると、混ぜた語が見出しから消える）
 */
import type { MediaFilter, MediaKind } from "./api.ts";

/** 条件の名前（辞書から渡す。この関数は文言を持たない） */
export interface FilterCondLabels {
  shelf: Record<Exclude<MediaFilter, "all">, string>;
  kind: Record<Exclude<MediaKind, "all">, string>;
  query: (q: string) => string;
  camera: (name: string) => string;
}

/** 左のカメラが入れる語（`App.tsx` の `camera:"${cam.name}"`）だけなら、その機種名 */
export function cameraOnly(query: string): string | null {
  const m = /^camera:"([^"]+)"$/.exec(query.trim());
  return m ? m[1] : null;
}

/** 効いている条件の名前を、左の一覧の上から並べる。何も効いていなければ空 */
export function filterConditions(
  filter: MediaFilter,
  kind: MediaKind,
  query: string,
  labels: FilterCondLabels,
): string[] {
  const out: string[] = [];
  if (filter !== "all") out.push(labels.shelf[filter]);
  if (kind !== "all") out.push(labels.kind[kind]);
  const q = query.trim();
  if (q !== "") {
    const camera = cameraOnly(q);
    out.push(camera !== null ? labels.camera(camera) : labels.query(q));
  }
  return out;
}

/**
 * 名前をつなぐ。**`App.tsx` の `nameList` と同じ作り**——3つまで出し、残りは
 * 「ほか N 件」と数で言う。いまの条件は棚・種類・検索語の最大3つなので畳まれないが、
 * 条件が増えたときに見出しが伸び続けないよう、同じ規則に揃えておく
 */
export function joinConditions(
  names: string[],
  separator: string,
  andMore: (n: number) => string,
): string {
  const shown = names.slice(0, 3);
  const rest = names.length - shown.length;
  return rest > 0 ? [...shown, andMore(rest)].join(separator) : shown.join(separator);
}
