/**
 * 動画の音量を覚える（pictkura-dev #54）。`<video>` は開くたびに作り直すので、何もしないと毎回最大音量で
 * 始まる——「たまに最大音量でびっくりする」（利用者、2026-10-17）。
 *
 * 音量と消音の両方を覚える（YouTube 等と同じ）。消音中（音量 0 も含む）はビューアに小さな印を出す。
 * 保存先は `localStorage`。値は**この窓の中にも持つ**——保存に失敗しても、開き直した動画が最大音量へ
 * 戻らないように（ゲート2）。保存先が使えない窓でも再生はでき、起動したては既定（最大・消音なし）
 */
const KEY = "pictkura.videoVolume";

export type VideoVolume = { volume: number; muted: boolean };

const fallback = (): VideoVolume => ({ volume: 1, muted: false });

/** 保存した値を読む。読めない・形が違う・範囲の外なら既定 */
export function parseVideoVolume(raw: string | null): VideoVolume {
  if (!raw) return fallback();
  try {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== "object" || v === null) return fallback();
    const { volume, muted } = v as Record<string, unknown>;
    if (typeof volume !== "number" || !Number.isFinite(volume) || volume < 0 || volume > 1) {
      return fallback();
    }
    return { volume, muted: muted === true };
  } catch {
    return fallback();
  }
}

/** 音が出ない状態か（消音、または音量 0）。印を出す条件 */
export function isSilent(v: VideoVolume): boolean {
  return v.muted || v.volume === 0;
}

/** この窓の中で最後に決まった値。保存先より先に見る */
let current: VideoVolume | null = null;

export function loadVideoVolume(): VideoVolume {
  if (current) return { ...current };
  try {
    current = parseVideoVolume(localStorage.getItem(KEY));
  } catch {
    current = fallback();
  }
  return { ...current };
}

/** 覚える。前と同じなら書かない（つまみを引く間は何十回も来る） */
export function saveVideoVolume(v: VideoVolume): void {
  if (current && current.volume === v.volume && current.muted === v.muted) return;
  current = { ...v };
  try {
    localStorage.setItem(KEY, JSON.stringify(v));
  } catch {
    // 覚えられなくても、この窓の中では `current` が効いている
  }
}
