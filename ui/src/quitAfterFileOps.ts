/**
 * ファイルを動かす操作（一覧・ビューアからの削除、フォルダへの移動）の間に押された×を、最後の1本が終わったときに
 * どう畳むか（pictkura-dev #69）。
 *
 * - `"stay"`: 閉じない。×が押されていない、または重なった操作のどれかが転んだ・残した（何が起きたかを読める場所に残す）
 * - `"handover"`: 関所の削除がまだ走っている。そちらが終えてから閉じる
 * - `"gate"`: ボツの印が（いま片付いたものを除いて）残っている。いつもどおり関所を出す
 * - `"destroy"`: 窓を閉じる
 */
export type QuitAfterFileOps = "stay" | "handover" | "gate" | "destroy";

export function quitAfterFileOps(s: {
  quitAsked: boolean;
  failed: boolean;
  gateTrashing: boolean;
  marks: Iterable<number>;
  gone: ReadonlySet<number>;
}): QuitAfterFileOps {
  if (!s.quitAsked || s.failed) return "stay";
  if (s.gateTrashing) return "handover";
  for (const id of s.marks) if (!s.gone.has(id)) return "gate";
  return "destroy";
}
