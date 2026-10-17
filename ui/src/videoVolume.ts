/**
 * 動画の音量を覚える（pictkura-dev #54）。`<video>` は開くたびに作り直すので、何もしないと毎回最大音量で
 * 始まる——「たまに最大音量でびっくりする」（利用者、2026-10-17）。
 *
 * 音量と消音の両方を覚える（YouTube 等と同じ）。消音は操作の帯のスピーカーの印に出ている。
 * 保存先は `localStorage`。使えない窓でも再生はでき、そのときは既定（最大・消音なし）
 */
const KEY = "pictkura.videoVolume";

export type VideoVolume = { volume: number; muted: boolean };

const DEFAULT: VideoVolume = { volume: 1, muted: false };

/** 保存した値を読む。読めない・形が違う・範囲の外なら既定 */
export function parseVideoVolume(raw: string | null): VideoVolume {
  if (!raw) return DEFAULT;
  try {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== "object" || v === null) return DEFAULT;
    const { volume, muted } = v as Record<string, unknown>;
    if (typeof volume !== "number" || !Number.isFinite(volume) || volume < 0 || volume > 1) {
      return DEFAULT;
    }
    return { volume, muted: muted === true };
  } catch {
    return DEFAULT;
  }
}

export function loadVideoVolume(): VideoVolume {
  try {
    return parseVideoVolume(localStorage.getItem(KEY));
  } catch {
    return DEFAULT;
  }
}

export function saveVideoVolume(v: VideoVolume): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(v));
  } catch {
    // 覚えられなくても、いま再生している動画には効いている
  }
}
