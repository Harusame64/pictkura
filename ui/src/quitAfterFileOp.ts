/**
 * ファイルを動かす操作（一覧・ビューアからの削除、書き出し・移動）の間に押された×を、終わったときにどう畳むか
 * （pictkura-dev #69。操作は同時に1本だけ）。
 *
 * - `"stay"`: 閉じない。転んだ・残した回（結果の文を読める場所に残す。関所の削除と同じ）
 * - `"destroy"`: 窓を閉じる。関所の上で押された×（「2度目」）は、印を見ずに閉じる
 * - `"gate"`: ボツの印が（いま片付いたものを除いて）残っている。いつもどおり関所を出す
 */
export type QuitAfterFileOp = "stay" | "gate" | "destroy";

export function quitAfterFileOp(s: {
  ok: boolean;
  pastGate: boolean;
  marks: Iterable<number>;
  gone: ReadonlySet<number>;
}): QuitAfterFileOp {
  if (!s.ok) return "stay";
  if (s.pastGate) return "destroy";
  for (const id of s.marks) if (!s.gone.has(id)) return "gate";
  return "destroy";
}
