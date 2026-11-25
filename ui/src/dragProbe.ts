/**
 * spike #76 only (not for main): splits the grid's dark area during a rail drag into
 * "day list not here yet" (placeholder rows) and "tile here, picture not drawn".
 * The pixel meter sees both as the same dark grey.
 *
 * Use over CDP: `__probeStart()`, drag, `__probeStop()` returns the samples.
 * Samples every 50 ms (not every frame) to keep layout reads from slowing the drag.
 */
type Sample = {
  t: number;
  scrollTop: number;
  /** share of the grid viewport */
  placeholder: number;
  tileBlank: number;
  tileShown: number;
  tiles: number;
};

function inter(a: DOMRect, b: DOMRect): number {
  const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
  const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
  return w > 0 && h > 0 ? w * h : 0;
}

let timer: number | null = null;
let samples: Sample[] = [];

function sample() {
  const sc = document.querySelector<HTMLElement>(".grid-scroll");
  if (!sc) return;
  const vp = sc.getBoundingClientRect();
  const area = vp.width * vp.height || 1;
  let placeholder = 0;
  let tileBlank = 0;
  let tileShown = 0;
  let tiles = 0;
  for (const el of sc.querySelectorAll<HTMLElement>(".placeholder-row")) {
    placeholder += inter(el.getBoundingClientRect(), vp);
  }
  for (const img of sc.querySelectorAll<HTMLImageElement>("img.cell")) {
    const a = inter(img.getBoundingClientRect(), vp);
    if (a === 0) continue;
    tiles++;
    const drawn = img.hasAttribute("data-shown") && img.complete && img.naturalWidth > 1;
    if (drawn) tileShown += a;
    else tileBlank += a;
  }
  samples.push({
    t: Math.round(performance.now()),
    scrollTop: Math.round(sc.scrollTop),
    placeholder: placeholder / area,
    tileBlank: tileBlank / area,
    tileShown: tileShown / area,
    tiles,
  });
}

const w = window as unknown as Record<string, unknown>;
w.__probeStart = () => {
  samples = [];
  if (timer != null) window.clearInterval(timer);
  timer = window.setInterval(sample, 50);
  return "started";
};
w.__probeStop = () => {
  if (timer != null) window.clearInterval(timer);
  timer = null;
  return JSON.stringify(samples);
};
