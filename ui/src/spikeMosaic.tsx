/** SPIKE（ThumbHash の代わり絵）。マージしない。日の一覧が届く前の行に、その日の写真のぼかしを並べる */
import { invoke } from "@tauri-apps/api/core";
import { thumbHashToDataURL } from "thumbhash";

type Light = { id: number; w: number; h: number; th: string };
export const spikeDays = new Map<number, Light[]>();
const urls = new Map<number, string>();
export let spikeStats = "";

export async function loadSpikeLight(): Promise<void> {
  const t0 = performance.now();
  const r = await invoke<{
    ids: number[]; days: number[]; w: number[]; h: number[]; th: string[];
    rows_ms: number; hash_ms: number; computed: number; cached: number;
  }>("spike_light");
  const t1 = performance.now();
  for (let i = 0; i < r.ids.length; i++) {
    const d = r.days[i];
    let a = spikeDays.get(d);
    if (!a) spikeDays.set(d, (a = []));
    a.push({ id: r.ids[i], w: r.w[i], h: r.h[i], th: r.th[i] });
  }
  spikeStats = `n=${r.ids.length} rows_ms=${r.rows_ms} hash_ms=${r.hash_ms} computed=${r.computed} cached=${r.cached} ipc_ms=${Math.round(t1 - t0)} build_ms=${Math.round(performance.now() - t1)}`;
  console.log("[spike]", spikeStats);
  (window as unknown as { __spike: string }).__spike = spikeStats;
}

function urlOf(it: Light): string | undefined {
  if (!it.th) return undefined;
  let u = urls.get(it.id);
  if (!u) {
    const bin = Uint8Array.from(atob(it.th), (c) => c.charCodeAt(0));
    u = thumbHashToDataURL(bin);
    urls.set(it.id, u);
  }
  return u;
}

export function SpikeMosaic({ day, height, rowH, gap }: { day: number; height: number; rowH: number; gap: number }) {
  const items = spikeDays.get(day);
  if (!items) return null;
  return (
    <div style={{ height, display: "flex", flexWrap: "wrap", gap, overflow: "hidden", alignContent: "flex-start" }}>
      {items.map((it) => {
        const u = urlOf(it);
        return (
          <div
            key={it.id}
            style={{
              height: rowH,
              width: Math.round((rowH * it.w) / it.h),
              flexGrow: 1,
              borderRadius: 3,
              background: u ? `center / cover url(${u})` : "var(--cell-pending)",
            }}
          />
        );
      })}
    </div>
  );
}
