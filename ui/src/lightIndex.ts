import type { MediaItem } from "./api";

/**
 * 一覧の軽い索引（pictkura-dev #76）。Rust の `light_index` が、検索・絞り込みに一致する写真**全部**の、並びと地の色に
 * 要る欄だけを固定長のバイト列で返す（形は `src-tauri` の `pack_light_index` の表。変えるなら両方と印を）。
 *
 * 一覧は、中身（`listDays`）がまだ来ていない日を、灰色の仮の行ではなく**本番と同じ形の行**で並べ、地に色を敷く。
 * 見えた日を取りに行くだけでは、レールを速く動かすと 1 日が 1 フレームも画面に居ず、間に合わなかった（spike）。
 * 止めた日の中身は今までどおり `listDays` が運び、本番のタイルに替わる——縦横比も重ねも同じ値なので、跳ねない。
 */
export const LIGHT_INDEX_MAGIC = 0x014c4b50;
export const LIGHT_RECORD_LEN = 112;
const HEAD_LEN = 8;
const TINY_AT = 64;

/** 索引から起こした項目。**一覧の並びと地の絵にしか使わない**（ファイル名・動画の長さなどは持たない） */
export type LightItem = MediaItem;

export interface LightIndex {
  /** 日ごとの項目（並びは `listDays` と同じ） */
  days: Map<number, LightItem[]>;
  /** 4x4 の色（RGBA 64 バイト）。色の無い写真は入らない */
  tiny: Map<number, Uint8ClampedArray<ArrayBuffer>>;
  /** id から項目（差分が並びを変えたかを見る） */
  byId: Map<number, LightItem>;
}

/** バイト列を読む。印・長さが合わなければ null（UI は索引を使わず、今までの読み方に戻る） */
export function decodeLightIndex(buf: ArrayBuffer): LightIndex | null {
  if (buf.byteLength < HEAD_LEN) return null;
  const v = new DataView(buf);
  if (v.getUint32(0, true) !== LIGHT_INDEX_MAGIC) return null;
  const n = v.getUint32(4, true);
  if (buf.byteLength !== HEAD_LEN + n * LIGHT_RECORD_LEN) return null;
  const bytes = new Uint8Array(buf);
  const days = new Map<number, LightItem[]>();
  const tiny = new Map<number, Uint8ClampedArray<ArrayBuffer>>();
  const byId = new Map<number, LightItem>();
  for (let i = 0; i < n; i++) {
    const o = HEAD_LEN + i * LIGHT_RECORD_LEN;
    const flags = v.getUint8(o + 61);
    const thumbState = v.getUint8(o + 60);
    const item: LightItem = {
      id: v.getFloat64(o, true),
      taken_at_ms: v.getFloat64(o + 8, true),
      mtime_ms: v.getFloat64(o + 16, true),
      shot_key: v.getFloat64(o + 24, true),
      body_key: v.getFloat64(o + 32, true),
      rev: v.getFloat64(o + 40, true),
      day_key: v.getInt32(o + 48, true),
      width: v.getUint32(o + 52, true),
      height: v.getUint32(o + 56, true),
      thumb_state: thumbState,
      is_raw: (flags & 1) !== 0,
      is_video: (flags & 2) !== 0,
      taken_at_known: (flags & 4) !== 0,
      taken_subsec: (flags & 8) !== 0,
      has_thumb: (flags & 16) !== 0,
      favorite: (flags & 32) !== 0,
      picked: (flags & 64) !== 0,
      // 索引に無い欄。軽いタイルは使わない
      file_name: "",
      preview_width: null,
      preview_height: null,
      duration_ms: null,
      plays_in_app: false,
      needs_transcode: false,
      is_jpeg: false,
    };
    if (v.getUint8(o + 62) === 1) {
      const px = new Uint8ClampedArray(64);
      for (let p = 0; p < 16; p++) {
        px[p * 4] = bytes[o + TINY_AT + p * 3];
        px[p * 4 + 1] = bytes[o + TINY_AT + p * 3 + 1];
        px[p * 4 + 2] = bytes[o + TINY_AT + p * 3 + 2];
        px[p * 4 + 3] = 255;
      }
      tiny.set(item.id, px);
    }
    byId.set(item.id, item);
    const day = days.get(item.day_key);
    if (day) day.push(item);
    else days.set(item.day_key, [item]);
  }
  return { days, tiny, byId };
}

/**
 * 一覧の並びを決める欄（版・日・縦横比と、重ねの組み方・表紙の選び方に効く欄）。`stacksOfDay` が読む欄を
 * 足したら、ここにも足す
 */
const LAYOUT_FIELDS = [
  "rev",
  "day_key",
  "width",
  "height",
  "taken_at_ms",
  "taken_at_known",
  "taken_subsec",
  "shot_key",
  "body_key",
  "is_raw",
  "is_video",
  // 連写の表紙は ⚑ の付いたコマ（表紙の縦横比がタイルの幅になる）
  "picked",
] as const satisfies readonly (keyof LightItem)[];

/**
 * 届いた差分（`media-updated`）が、索引で並べた形を変えるか。変えるなら索引を読み直す。
 * サムネイルの段階だけの差分は変えない（タイルの絵は本番の行が届けば替わる）——サムネイルを作っている間、
 * 何千回も読み直さない。索引に居ない写真（「読み込み中」の束から出てきた写真）は、骨組みの枚数のずれで拾う
 */
export function changesLightLayout(index: LightIndex | null, p: Pick<LightItem, (typeof LAYOUT_FIELDS)[number] | "id">): boolean {
  const it = index?.byId.get(p.id);
  if (!it) return false;
  return LAYOUT_FIELDS.some((k) => it[k] !== p[k]);
}

/**
 * ⚑ を付け替えた写真を索引にも写す（連写の表紙は ⚑ のコマなので、表紙の縦横比が変わりうる）。⚑ の付け替えには
 * Rust から差分の知らせが来ないので、ここで写さないと、その日が一覧の控えから外れた後に古い表紙で並ぶ（PR #367 の
 * codex）。変わる写真が無ければ同じ索引を返す
 */
export function withPicked(index: LightIndex, ids: Iterable<number>, picked: boolean): LightIndex {
  const changed = new Map<number, LightItem>();
  for (const id of ids) {
    const it = index.byId.get(id);
    if (it && it.picked !== picked) changed.set(id, { ...it, picked });
  }
  if (changed.size === 0) return index;
  const days = new Map(index.days);
  const byId = new Map(index.byId);
  const touchedDays = new Set([...changed.values()].map((it) => it.day_key));
  for (const day of touchedDays) {
    days.set(day, (index.days.get(day) ?? []).map((it) => changed.get(it.id) ?? it));
  }
  for (const [id, it] of changed) byId.set(id, it);
  return { days, tiny: index.tiny, byId };
}

/**
 * 索引の日を一覧に使ってよいか。**骨組みの枚数と合う日だけ**——合わない日（索引を読んだ後に写真が入った・
 * 移った）を索引で並べると、本番の行が届いたときに跳ねる。合わない日は今までどおり仮の行で待つ
 */
export function usableLightDays(
  days: ReadonlyMap<number, readonly LightItem[]>,
  summary: readonly { day_key: number; count: number }[],
): Set<number> {
  const out = new Set<number>();
  for (const d of summary) {
    if (days.get(d.day_key)?.length === d.count) out.add(d.day_key);
  }
  return out;
}
