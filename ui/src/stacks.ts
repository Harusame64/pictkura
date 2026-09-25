/**
 * 一覧の「重ね」（dev #32、`dev/adr.grid-stacks.md`）。純関数。`scripts/stacks.test.ts` が表で見る。
 *
 * 形は2層:
 * - **コマ（`Shot`）** = 同じ撮影の組。同じフォルダ・同じ名前の RAW+JPEG（Rust の `pair_key`）。
 *   組でなければ1ファイル
 * - **重ね（`Stack`）** = 一覧のタイル1枚。いまは常に1コマ——連写（続けて撮ったコマの並び）は
 *   次の PR でここに足す。型を先に2段にしておくのはそのため
 *
 * **束ねるのは RAW を含む組だけ**。同じ名前というだけで束ねると、iPhone の Live Photos
 * （`IMG_0001.HEIC` + `IMG_0001.MOV`）で動画が隠れる。
 * **一覧の描き方だけを変える**——ビューアは今までどおり1ファイルずつ歩く
 * （2026-09-08 の利用者の依頼「詳細ページは逐次でよい」）。
 */

/** 重ねを組むのに要る欄だけ（`MediaItem` の部分集合。試験で小さく作れるように） */
export interface Stackable {
  id: number;
  shot_key: number;
  is_raw: boolean;
}

/** コマ: `lead` が一覧に出る1枚、`files` は組ぜんぶ（`lead` を含む・日の並び順） */
export type Shot<T> = { lead: T; files: T[] };
/** 重ね: 一覧のタイル1枚。`cover` が表紙のコマ */
export type Stack<T> = { cover: Shot<T>; shots: Shot<T>[] };

export interface StackOptions {
  /** RAW+JPEG を1コマに重ねるか（設定 `[grid] stack_raw_jpeg`） */
  rawJpeg: boolean;
}

/**
 * 1日ぶんの並び（`list_day` の順）を重ねに組む。
 *
 * - 重ねの位置は、**組の最初の1件が居た位置**（並び順を崩さない）
 * - 表紙は **RAW でない1枚**（JPEG。サムネイルが既に在るので速い——2026-09-08 の利用者の選択）。
 *   無ければ先頭
 * - `list_day` は**その日の全件**を返すので、組がページの境目で割れることは無い
 */
export function stacksOfDay<T extends Stackable>(
  items: readonly T[],
  opts: StackOptions,
): Stack<T>[] {
  const single = (it: T): Stack<T> => {
    const shot = { lead: it, files: [it] };
    return { cover: shot, shots: [shot] };
  };
  if (!opts.rawJpeg) return items.map(single);

  const groups = new Map<number, T[]>();
  for (const it of items) {
    const g = groups.get(it.shot_key);
    if (g) g.push(it);
    else groups.set(it.shot_key, [it]);
  }
  const out: Stack<T>[] = [];
  const emitted = new Set<number>();
  for (const it of items) {
    const g = groups.get(it.shot_key) ?? [it];
    const stacked = g.length > 1 && g.some((f) => f.is_raw);
    if (!stacked) {
      out.push(single(it));
      continue;
    }
    if (emitted.has(it.shot_key)) continue;
    emitted.add(it.shot_key);
    const shot = { lead: g.find((f) => !f.is_raw) ?? g[0], files: g };
    out.push({ cover: shot, shots: [shot] });
  }
  return out;
}

/** 重ねに含まれるファイルぜんぶ（★・⚑・削除・選択は**組ぜんぶに効く**——2026-09-08 の利用者の選択） */
export function filesOf<T>(stack: Stack<T>): T[] {
  return stack.shots.flatMap((s) => s.files);
}

/**
 * id → その id が居る重ねのファイルの id ぜんぶ（重なっていない id は入れない）。
 * 範囲選択（Shift）の閉包に使う
 */
export function stackMembersIndex<T extends Stackable>(
  stacks: readonly Stack<T>[],
): Map<number, number[]> {
  const index = new Map<number, number[]>();
  for (const st of stacks) {
    const ids = filesOf(st).map((f) => f.id);
    if (ids.length < 2) continue;
    for (const id of ids) index.set(id, ids);
  }
  return index;
}

/**
 * 選んだ id の集合を、**重ねの単位へ閉じる**（どれか1つが入っている重ねは、全部を入れる）。
 *
 * 範囲選択は DB から id の範囲を受け取るので、**組の途中で切れうる**——切れたまま消すと、
 * RAW が独りで一覧に残る。**範囲の端は必ず読み込み済みの日に在る**（利用者がその2枚を
 * クリックしたのだから）。**間に挟まる日は丸ごと入る**ので、組が割れるのは両端だけ。
 * だから読み込み済みの日から作った索引で閉じれば、**完全に塞がる**（旧 ADR の論証）
 */
export function closeOverStacks(
  ids: Iterable<number>,
  index: ReadonlyMap<number, readonly number[]>,
): Set<number> {
  const out = new Set<number>();
  for (const id of ids) {
    out.add(id);
    for (const m of index.get(id) ?? []) out.add(m);
  }
  return out;
}

/**
 * 選んだ id が**見えているタイル何枚ぶん**か（dev #32）。重ねの組は1枚と数える。
 * 選択中の枚数と削除の確認文に使う——1枚の重ねを選んで「2枚を選択中」と言わない
 */
export function countPhotos(
  ids: Iterable<number>,
  index: ReadonlyMap<number, readonly number[]>,
): number {
  const seen = new Set<number>();
  for (const id of ids) seen.add(index.get(id)?.[0] ?? id);
  return seen.size;
}
