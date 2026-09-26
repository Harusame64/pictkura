/**
 * 一覧の「重ね」（dev #32、`dev/adr.grid-stacks.md`）。純関数。`scripts/stacks.test.ts` が表で見る。
 *
 * 形は2層:
 * - **コマ（`Shot`）** = 同じ撮影の組。同じフォルダ・同じ名前の RAW+JPEG（Rust の `pair_key`）。
 *   組でなければ1ファイル
 * - **重ね（`Stack`）** = 一覧のタイル1枚。1コマか、**連写**（同じ機体で続けて撮ったコマの並び）。
 *   2つは独立していて、RAW+JPEG で連写すれば「連写 12 コマ、各コマ RAW+JPEG」になる
 *
 * **連写の規則（ADR の「A」、2026-09-25 の利用者の選択）**: 次の全部を満たすコマを鎖でつなぐ。
 * 1. 同じ機体（`body_key`＝機種＋本体シリアル。0＝分からないものは束ねない）
 * 2. 秒未満の撮影時刻がある（`taken_subsec`。秒までしか分からないコマは束ねない——
 *    誤って束ねるより、束ねない）
 * 3. **同じ機体の直前のコマ**との間隔が設定値以下（既定1秒）
 *
 * 「直前」は**機体ごとの撮影時刻の順**で見る——一覧の隣ではない。2台で同じ時間帯に撮ると、
 * 一覧ではコマが交互に並ぶが、それぞれの機体の連写は割れない。束は日ごとに組む（日をまたぐと分かれる）
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
 * **ビューアの歩き方は `viewerWalk` が別に組む**。2026-09-08 は「詳細ページは逐次でよい」
 * （1ファイルずつ）だったが、2026-09-26 に組の片方だけ（既定は JPEG）・RAW→JPEG を選べるようにした
 * ——**組の条件（`shotsOfDay`）を変えると、全画面で隠れるファイルと印の効き先も変わる**。
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
  /** `taken_at_ms` が秒未満まで分かっているか（連写の条件） */
  taken_subsec: boolean;
  /** 機体の鍵（機種＋本体シリアル。0 は分からない） */
  body_key: number;
  /** 選別の印（⚑）。連写の表紙は ⚑ のコマ */
  picked: boolean;
}

/** コマ: `lead` が一覧に出る1枚、`files` は組ぜんぶ（`lead` を含む・日の並び順） */
export type Shot<T> = { lead: T; files: T[] };
/**
 * 重ね: 一覧のタイル1枚。`cover` が表紙のコマ。`spanMs` は連写の最初のコマから最後のコマまで
 * （鎖に使った秒未満の時刻で。連写でなければ無い）
 */
export type Stack<T> = { cover: Shot<T>; shots: Shot<T>[]; spanMs?: number };

export interface StackOptions {
  /** RAW+JPEG を1コマに重ねるか（設定 `[grid] stack_raw_jpeg`） */
  rawJpeg: boolean;
  /** 連写を1枚に重ねるか（設定 `[grid] stack_bursts`）。省くと重ねない */
  bursts?: boolean;
  /** 連写とみなす間隔の上限（ミリ秒。設定 `[grid] burst_gap_ms`）。省くと 1000 */
  burstGapMs?: number;
}

/**
 * 1日ぶんの並び（`list_day` の順）を重ねに組む。
 *
 * - 重ねの位置は、**組の最初の1件が居た位置**（並び順を崩さない）
 * - コマの表紙は **RAW でない1枚**（JPEG。サムネイルが既に在るので速い——2026-09-08 の利用者の選択）。
 *   無ければ先頭
 * - 連写の表紙は **⚑ のコマ**（いくつもあれば撮り始めに近いもの）。無ければ**撮り始めのコマ**
 * - `list_day` は**その日の全件**を返すので、組がページの境目で割れることは無い
 */
export function stacksOfDay<T extends Stackable>(
  items: readonly T[],
  opts: StackOptions,
): Stack<T>[] {
  const single = (shot: Shot<T>): Stack<T> => ({ cover: shot, shots: [shot] });
  if (!opts.bursts) return shotsOfDay(items, opts.rawJpeg).map(single);

  // **連写の中のコマは、RAW+JPEG の重ねの設定によらず組で数える**（#160 のゲート2）。
  // 設定を切ったときに組をばらばらのコマにすると、同じシャッターの RAW と JPEG が間隔0で
  // つながり、連写でない1組が「連写 2」になる（マニュアルの「切れば別々に出る」に反する）。
  // 設定が効くのは**連写にならなかった組**だけ——切っていれば、そこでファイルごとに分ける
  const shots = shotsOfDay(items, true);

  const gap = opts.burstGapMs ?? 1000;
  // 機体ごとに、撮影時刻の順に並べて鎖を切る。`at` は一覧の位置（重ねを置く場所）
  const byBody = new Map<number, { shot: Shot<T>; at: number; ms: number }[]>();
  shots.forEach((shot, at) => {
    const timed = burstTime(shot);
    if (timed === null) return;
    const list = byBody.get(timed.body);
    const entry = { shot, at, ms: timed.ms };
    if (list) list.push(entry);
    else byBody.set(timed.body, [entry]);
  });
  /** 一覧の位置 → そこに置く連写（連写の最初の1コマの位置だけ）。他のコマの位置は空ける */
  const placed = new Map<number, Stack<T>>();
  const absorbed = new Set<number>();
  for (const list of byBody.values()) {
    list.sort((a, b) => a.ms - b.ms || a.at - b.at);
    let run = [list[0]];
    const flush = () => {
      if (run.length >= 2) {
        const byPlace = [...run].sort((a, b) => a.at - b.at);
        // 表紙: ⚑ のコマ（撮り始めに近いもの）、無ければ撮り始め。`run` は撮影時刻の順
        const cover = (run.find((e) => e.shot.files.some((f) => f.picked)) ?? run[0]).shot;
        placed.set(byPlace[0].at, {
          cover,
          shots: byPlace.map((e) => e.shot),
          spanMs: run[run.length - 1].ms - run[0].ms,
        });
        for (const e of byPlace) absorbed.add(e.at);
      }
    };
    for (let k = 1; k < list.length; k++) {
      if (list[k].ms - list[k - 1].ms <= gap) run.push(list[k]);
      else {
        flush();
        run = [list[k]];
      }
    }
    flush();
  }
  // **並べる位置は元の並びの位置**。重ねは最初のファイルの位置に置き、ばらした組の
  // ファイルはそれぞれ自分の位置へ戻す——組のそばにまとめて出すと、同じ秒に撮った別の写真が
  // 組の間に挟まっていたとき並びが崩れ、範囲選択が見えているタイルを飛ばす（#160 の codex）
  const indexOf = new Map<T, number>();
  items.forEach((it, i) => indexOf.set(it, i));
  // 展開（`Math.min(...)`）にしない——長い連写は引数の数の上限に届く
  const first = (files: readonly T[]) =>
    files.reduce((m, f) => Math.min(m, indexOf.get(f) ?? 0), Infinity);
  const placedAt: [number, Stack<T>][] = [];
  shots.forEach((shot, at) => {
    const burst = placed.get(at);
    if (burst) placedAt.push([first(filesOf(burst)), burst]);
    else if (absorbed.has(at)) return;
    else if (opts.rawJpeg || shot.files.length === 1) placedAt.push([first(shot.files), single(shot)]);
    else for (const f of shot.files) placedAt.push([indexOf.get(f) ?? 0, single({ lead: f, files: [f] })]);
  });
  return placedAt.sort((a, b) => a[0] - b[0]).map(([, st]) => st);
}

/**
 * 連写の鎖に入れる時刻と機体。**入れないコマは `null`**: 機体が分からない・秒未満が無い・
 * 撮影日時が読めていない・動画。RAW+JPEG のコマは、秒未満を持つ方の時刻を使う
 * （JPEG は秒未満を持ち、読み直していない RAW は秒までのことがある——#158）
 */
function burstTime<T extends Stackable>(shot: Shot<T>): { body: number; ms: number } | null {
  const f = shot.files.find(
    (x) => x.body_key !== 0 && x.taken_subsec && x.taken_at_known && !x.is_video,
  );
  return f ? { body: f.body_key, ms: f.taken_at_ms } : null;
}

/** 1日ぶんの並びをコマに組む（一覧の順。コマの位置は組の最初の1件が居た位置） */
function shotsOfDay<T extends Stackable>(items: readonly T[], rawJpeg: boolean): Shot<T>[] {
  const single = (it: T): Shot<T> => ({ lead: it, files: [it] });
  if (!rawJpeg) return items.map(single);

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
  const out: Shot<T>[] = [];
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
    out.push({ lead: g.find((f) => !f.is_raw) ?? g[0], files: g });
  }
  return out;
}

/** ビューアでの RAW+JPEG の組の歩き方（設定 `[viewer] pair_view`、2026-09-26 の利用者の選択） */
export type PairView = "jpeg" | "raw" | "both";

/**
 * ビューアが1日の中で歩く列（`walk`）と、組の相手（`pairOf`）。
 *
 * - `jpeg` / `raw`: 組は**片方だけ**を列に入れる（一覧のタイル1枚につき絵1枚）
 * - `both`: 組を **RAW → JPEG の順**に並べる（`list_day` の順では id しだいで前後した）
 * - 組の位置は、一覧と同じく**組の最初の1件が居た位置**（[`stacksOfDay`] と同じ組み方）
 * - 組にならないファイル（組の条件は [`stacksOfDay`] の説明）は、そのまま自分の位置に居る
 *
 * `pairOf` は組のファイル id → 組ぜんぶ（RAW が先）。**列に居ない側の id も引ける**——
 * 一覧のタイルから開くと表紙の JPEG の id が来るので、`raw` のときはそこから RAW へ寄せる。
 * 片方だけ見せている間の ★・⚑・✕・削除も、これで組の両方に効かせる
 */
export function viewerWalk<T extends Stackable>(
  items: readonly T[],
  view: PairView,
): { walk: T[]; pairOf: Map<number, T[]> } {
  const walk: T[] = [];
  const pairOf = new Map<number, T[]>();
  for (const shot of shotsOfDay(items, true)) {
    if (shot.files.length === 1) {
      walk.push(shot.files[0]);
      continue;
    }
    const raws = shot.files.filter((f) => f.is_raw);
    const others = shot.files.filter((f) => !f.is_raw);
    const ordered = [...raws, ...others];
    for (const f of ordered) pairOf.set(f.id, ordered);
    if (view === "both") walk.push(...ordered);
    // 組は RAW と RAW 以外が必ずそろう（`shotsOfDay`）。同じ側が2つ以上あれば（CR3 と
    // それを書き出した DNG 等）**その側は全部**見せる——隠すのは選ばなかった側だけ
    else walk.push(...(view === "raw" ? raws : others));
  }
  return { walk, pairOf };
}

/**
 * 選んだ写真だけを歩く列（ビューアの選択スコープ）を、組の歩き方にそろえる（#168 の codex）。
 *
 * 選択の列はファイルの並び（`list_day` の順）なので、そのままだと `both` でも JPEG が先に来たり、
 * 片方だけのときに隠れた側の席が残って、端の判定・隣の先読みが途切れたりする。
 * 組は**その組の最初の1件が居た席**に、見せる側だけを RAW が先の順で置く。
 * `pairOf` に無い id（組でない・その日をまだ読んでいない）はそのまま
 */
export function pairAwareScope<E extends { id: number }>(
  scope: readonly E[],
  pairOf: ReadonlyMap<number, readonly { id: number; is_raw: boolean }[]>,
  view: PairView,
): E[] {
  const shown = (f: { is_raw: boolean }) => view === "both" || (view === "raw") === f.is_raw;
  const out: E[] = [];
  const seen = new Set<number>();
  for (const e of scope) {
    if (seen.has(e.id)) continue;
    const pair = pairOf.get(e.id);
    if (!pair) {
      seen.add(e.id);
      out.push(e);
      continue;
    }
    for (const f of pair) seen.add(f.id);
    for (const f of pair) if (shown(f)) out.push({ ...e, id: f.id });
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
 * 範囲選択（Shift）を**タイルの並び**で切る（dev #32、#160 の codex の P2）。
 *
 * DB が返す範囲は**ファイルの並び**なので、連写のように**一覧で飛び飛びに並ぶ重ね**
 * （別の機体の写真が間に挟まる）では、範囲の外に置かれたタイルのコマが範囲の中に居る——
 * それを `closeOverStacks` で閉じると、**選んでいないタイルが丸ごと入る**
 * （`A1, B, A2, C` はタイル `A, B, C`。B〜C を選ぶと DB は `B, A2, C` を返し、A が入る）。
 *
 * `tilePos` は**読み込み済みの日**の「id → 一覧でのタイルの通し番号」。両端のタイルの間に
 * 置かれたタイルを、中身ぜんぶで入れる（同じタイルの id は同じ番号）。**読み込んでいない日の id は
 * DB の範囲からそのまま入れる**
 * ——そこは両端の間に挟まる日で、日ごと丸ごと範囲に入る（重ねは日ごとに組むので日をまたがない）。
 * 両端のどちらかが読み込み済みでなければ、ファイルの並びの閉包に落とす
 */
export function selectRangeOverTiles(
  range: Iterable<number>,
  fromId: number,
  toId: number,
  tilePos: ReadonlyMap<number, number>,
  index: ReadonlyMap<number, readonly number[]>,
): Set<number> {
  const a = tilePos.get(fromId);
  const b = tilePos.get(toId);
  if (a === undefined || b === undefined) return closeOverStacks(range, index);
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  const out = new Set<number>();
  // **読み込み済みの日は、タイルの位置で数え上げる**（DB の範囲を絞るのではなく）。起点が
  // 重ね方の変わる前に選ばれていると、起点は連写の途中のコマになりうる——DB の範囲はそのファイルの
  // 位置から始まるので、見えている間のタイルが範囲に入らない（#160 の codex、3周目）
  for (const [id, p] of tilePos) if (p >= lo && p <= hi) out.add(id);
  // 読み込んでいない日（両端の間に挟まる日）だけを DB の範囲で補う
  for (const id of range) if (!tilePos.has(id)) out.add(id);
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

/**
 * 間引かない日: **選んだ写真が居る日**。選択の枚数は、選んだ写真の日が読み込まれている間だけ
 * タイルで数えられる（`countPhotos`）。組のタイルを1つ選んで遠くへスクロールし、その日が間引かれると、
 * 帯の「1枚を選択中」がファイルの数の「2枚」に変わった（Windows の実機。2026-09-26）。
 * 削除の確認文も同じ数え方を使う。
 *
 * **守るのは `cap` 日まで**（一覧の日のキャッシュの上限）。全選択のまま端から端までスクロールすると、守る日が
 * 際限なく増えてキャッシュの上限が効かなくなる——超えたら守らない（今までどおりファイルの数に落ちる）
 */
export function selectedDaysIn(
  days: ReadonlyMap<number, readonly { id: number }[]>,
  selected: ReadonlySet<number>,
  cap: number,
): Set<number> {
  const out = new Set<number>();
  if (selected.size === 0) return out;
  for (const [key, items] of days) {
    if (items.some((it) => selected.has(it.id))) out.add(key);
    if (out.size > cap) return new Set();
  }
  return out;
}
