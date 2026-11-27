/**
 * 自作の再生バーの決め方（dev #58、dev/plan.video-controls.md）。標準の再生バーは、Windows（WebView2）で
 * 音量のつまみ・シークバーに焦点があると ← → と Esc を頁に一切渡さなかった（win の実機 2 周）。キーと焦点を
 * こちらで持つために、バーを自分で描く。ここは画面に依らない計算だけを置き、VideoBar.tsx と App.tsx が使う。
 */

/** 10 秒戻す・送るボタンの幅（標準のバーと同じ） */
export const SKIP_SECONDS = 10;
/** Shift+← → の幅（利用者の選択） */
export const SEEK_KEY_SECONDS = 5;
/** ↑ ↓ の音量の刻み（利用者の選択） */
export const VOLUME_STEP = 0.05;
/** 再生速度の候補（標準の「その他」メニューと同じ並び） */
export const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2] as const;

/** 秒を「m:ss」か「h:mm:ss」に。長さが分からない（NaN・Infinity）なら「--:--」 */
export function formatTime(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) return "--:--";
  const s = Math.floor(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** 長さが分かっていてシークできるか（MediaRecorder で作った webm は Infinity のまま。標準のバーもシークバーを出さない） */
export function canSeek(duration: number): boolean {
  return Number.isFinite(duration) && duration > 0;
}

/** シークの行き先。0 と長さの間に収める（長さが分からなければ動かさない） */
export function seekTarget(current: number, delta: number, duration: number): number {
  if (!canSeek(duration)) return current;
  return Math.min(duration, Math.max(0, current + delta));
}

/** 音量の行き先。0〜1 に収め、刻みの誤差（0.1 + 0.05 = 0.15000000000000002）を丸める */
export function volumeTarget(current: number, delta: number): number {
  const v = Math.min(1, Math.max(0, current + delta));
  return Math.round(v * 100) / 100;
}

/** 音を上げたとき、ミュートは解く（標準のバーと同じ）。音量 0 から上げたときも聞こえるように */
export function afterVolumeKey(state: { volume: number; muted: boolean }, delta: number): { volume: number; muted: boolean } {
  const volume = volumeTarget(state.volume, delta);
  return { volume, muted: delta > 0 ? false : state.muted };
}

/** 速度の表示（1 は呼ぶ側が「標準」と出す） */
export function formatSpeed(rate: number): string {
  return `${rate}×`;
}
