/**
 * 一覧の「重ね」（dev #32、`dev/adr.grid-stacks.md`）。純関数。`scripts/stacks.test.ts` が表で見る。
 *
 * 形は2層:
 * - **コマ（`Shot`）** = 同じ撮影の組。同じフォルダ・同じ名前の RAW+JPEG（Rust の `pair_key`）。
 *   組でなければ1ファイル
 * - **重ね（`Stack`）** = 一覧のタイル1枚。いまは常に1コマ——連写（続けて撮ったコマの並び）は
 *   次の PR でここに足す。型を先に2段にしておくのはそのため
 *
 * **束ねるのは RAW と RAW 以外がそろった組だけ**。同じ名前というだけで束ねると、iPhone の
 * Live Photos（`IMG_0001.HEIC` + `IMG_0001.MOV`）で動画が隠れる。RAW 同士（CR3 と、それを
 * 書き出した DNG）は「RAW+JPEG」ではないので束ねない。
 *
 * **名前だけでは同じ撮影と言えない**——カメラ2台で、片方は RAW だけ（`IMG_0001.CR3`）・
 * もう片方は JPEG だけ（`IMG_0001.JPG`）で撮ると、名前が同じ別の写真になる（#156 のゲート2）。
 * 組にするのは**撮影日時も同じもの**だけ。同じシャッターの RAW と JPEG は同じ日時を持つ。
 * **撮影日時が読めていない**（mtime で埋めた）ものは組に入れない——mtime は粗い媒体で
 * 偶然そろう（PRのcodex）。読めるまでは束ねない側に倒す。
 * **動画は組に入れない**（RAW と同名の動画を隠さない。PRのcodex）
 * **一覧の描き方だけを変える**——ビューアは今までどおり1ファイルずつ歩く
 * （2026-09-08 の利用者の依頼「詳細ページは逐次でよい」）。
 */

/** 重ねを組むのに要る欄だけ（`MediaItem` の部分集合。試験で小さく作れるように） */
export interface Stackable {
  id: number;
  shot_key: number;
  is_raw: boolean;
  /** 撮影日時（無ければ mtime）。**同じシャッターの RAW と JPEG は一致する** */
  taken_at_ms: number;
  /** `taken_at_ms` が本物の撮影日時か（偽なら mtime で埋めた値） */
  taken_at_known: boolean;
  /** 動画か（組に入れない——`IMG_0001.CR3` と `IMG_0001.MOV` を1枚にしない） */
  is_video: boolean;
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

  // **撮影日時は秒の単位で比べる**（dev #32 の #158 から、JPEG には秒未満が付く）。CR3 と JPEG で
  // 秒未満の有無がそろわない（CR3 を読み直していない・片方だけ OS から秒までの日時を借りた）と、
  // ミリ秒の違いで組が割れる
  const keyOf = (it: T) => `${it.shot_key}:${Math.floor(it.taken_at_ms / 1000)}`;
  const eligible = (it: T) => it.taken_at_known && !it.is_video;
  const groups = new Map<string, T[]>();
  for (const it of items) {
    if (!eligible(it)) continue;
    const g = groups.get(keyOf(it));
    if (g) g.push(it);
    else groups.set(keyOf(it), [it]);
  }
  const out: Stack<T>[] = [];
  const emitted = new Set<string>();
  for (const it of items) {
    if (!eligible(it)) {
      out.push(single(it));
      continue;
    }
    const key = keyOf(it);
    const g = groups.get(key) ?? [it];
    const stacked = g.some((f) => f.is_raw) && g.some((f) => !f.is_raw);
    if (!stacked) {
      out.push(single(it));
      continue;
    }
    if (emitted.has(key)) continue;
    emitted.add(key);
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
 * 選択中の枚数と、削除・移動の確認文と報告に使う——1枚の重ねを選んで「2枚」と言わない。
 *
 * **読み込んでいない日の id が混ざっていたら `null`**（数えられない）。その日の組は索引に
 * 無いので、ファイルの数を「枚数」と言ってしまい、スクロールで日が読み込まれるたびに
 * 数が変わって見える（#156 のゲート2）。呼ぶ側は `null` ならファイルの数で言う
 */
export function countPhotos(
  ids: Iterable<number>,
  index: ReadonlyMap<number, readonly number[]>,
  loaded: ReadonlySet<number>,
): number | null {
  const seen = new Set<number>();
  for (const id of ids) {
    if (!loaded.has(id)) return null;
    seen.add(index.get(id)?.[0] ?? id);
  }
  return seen.size;
}
