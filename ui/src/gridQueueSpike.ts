/**
 * SPIKE（列で選ぶ）: 一覧のサムネイルを TileLoader（取り込み画面と同じ列）で読む。マージ前に作り直す。
 *   pk.spike.q   1 で有効（既定 0 = 今まで通り <img src>）
 *   pk.spike.q.n 同時に読む数（既定 6）
 * window.__spikeBench(count, conc, offset) で排水口の太さ（1秒に何枚読めるか）を測る
 */
import { TileLoader } from "./useTileQueue";
import { listMediaIds, thumbSrcOf } from "./api";

function num(key: string, dflt: number): number {
  try {
    const v = localStorage.getItem(key);
    const n = v === null ? NaN : Number(v);
    return Number.isFinite(n) ? n : dflt;
  } catch {
    return dflt;
  }
}
export const QSPIKE = { on: num("pk.spike.q", 0) === 1, n: num("pk.spike.q.n", 6) };
export const gridLoader = new TileLoader(QSPIKE.n);
export const qCounters = { started: 0, finished: 0 };
/** 読み終えた URL（同じ URL は WebView のキャッシュから出る） */
export const loadedThumbs = new Set<string>();

const refCallbacks = new Map<string, (el: HTMLImageElement | null) => void>();
/** タイルの <img> に付ける ref。URL ごとに同じ関数を返す（作り直すと付け外しが毎回起きる） */
export function gridThumbRef(url: string): (el: HTMLImageElement | null) => void {
  let cb = refCallbacks.get(url);
  if (cb) return cb;
  let detach: (() => void) | null = null;
  cb = (el) => {
    detach?.();
    detach = null;
    if (!el || loadedThumbs.has(url)) return;
    qCounters.started++;
    detach = gridLoader.attach(url, el, () => {
      qCounters.finished++;
      loadedThumbs.add(url);
      if (el.isConnected) el.src = url;
    });
  };
  refCallbacks.set(url, cb);
  if (refCallbacks.size > 5000) refCallbacks.clear();
  return cb;
}

function pct(a: number[], p: number): number {
  if (a.length === 0) return 0;
  const s = [...a].sort((x, y) => x - y);
  return Math.round(s[Math.min(s.length - 1, Math.floor((s.length * p) / 100))] * 10) / 10;
}

/** 排水口の太さ: count 枚を conc 本ずつ、キャッシュを外して読む */
async function bench(count = 400, conc = 8, offset = 0) {
  const ids = (await listMediaIds("", "all")).slice(offset, offset + count);
  const tag = `bench${Date.now()}`;
  const lat: number[] = [];
  const decode: number[] = [];
  let next = 0;
  const t0 = performance.now();
  const worker = async () => {
    while (next < ids.length) {
      const id = ids[next++];
      const url = thumbSrcOf(id, 0, 0, 0) + `&${tag}`;
      const s = performance.now();
      const img = new Image();
      img.src = url;
      try {
        await new Promise<void>((res, rej) => {
          img.onload = () => res();
          img.onerror = () => rej(new Error("x"));
        });
        lat.push(performance.now() - s);
        const d = performance.now();
        await img.decode().catch(() => {});
        decode.push(performance.now() - d);
      } catch {
        /* 失敗も数に入れない */
      }
    }
  };
  await Promise.all(Array.from({ length: conc }, worker));
  const wall = performance.now() - t0;
  const entries = performance
    .getEntriesByType("resource")
    .filter((e) => e.name.includes(tag)) as PerformanceResourceTiming[];
  const server = entries.map((e) => e.responseStart - e.requestStart).filter((x) => x > 0);
  const transfer = entries.map((e) => e.responseEnd - e.responseStart).filter((x) => x >= 0);
  const r = {
    count: lat.length,
    conc,
    wallMs: Math.round(wall),
    perSec: Math.round((lat.length / wall) * 1000),
    latP50: pct(lat, 50),
    latP95: pct(lat, 95),
    serverP50: pct(server, 50),
    serverP95: pct(server, 95),
    transferP50: pct(transfer, 50),
    decodeP50: pct(decode, 50),
    decodeP95: pct(decode, 95),
    timingEntries: entries.length,
  };
  console.log("[bench]", JSON.stringify(r));
  return r;
}
(window as unknown as Record<string, unknown>).__spikeBench = bench;
(window as unknown as Record<string, unknown>).__spikeQ = { params: QSPIKE, counters: qCounters };
