/**
 * 取り込みの画面を開いたときに、どのドライブを最初から選んでおくか（pictkura-dev #39）の**判定だけ**。
 *
 * 利用者の声（2026-10-10）:「開いた時点でリムーバブルドライブ（特にSDカード）を選択状態とするか、
 * もう少し存在感を目立たせた方が良い」。**候補がちょうど1つのときだけ選び、複数なら目立たせるだけ**にする。
 *
 * **候補は「`DCIM` を持つドライブ」を先に見る。** macOS では外付けのバックアップ用ディスクも
 * `removable` と申告され、SD カードと見分けが付かない（`list_drives` の注）——それを選ぶと、
 * 開いただけでバックアップ用ディスクを丸ごと走査する。`DCIM` はカメラのカードが必ず持つ確かな合図である。
 * `DCIM` を持つものが無いときだけ、**Windows の取り外せるドライブ**（`GetDriveTypeW` の判定で、USB メモリや
 * カード読み取り機）を候補にする。macOS の `removable` はこの段では使わない。
 *
 * React も `window` も持ち込まない（`scripts/importPreselect.test.ts` から `node --test` で読む）。
 */

/** 判定に要るドライブの形（`api.ts` の `DriveInfo` の一部） */
export interface DriveLike {
  path: string;
  kind: string;
  dcim_path: string | null;
}

/** 最初から選んでおくドライブ。候補がちょうど1つのときだけ返す。無ければ・複数なら null */
export function preselectDrive<D extends DriveLike>(drives: readonly D[], platform: string): D | null {
  const cards = drives.filter((d) => d.dcim_path !== null);
  const candidates =
    cards.length > 0 ? cards : platform === "windows" ? drives.filter((d) => d.kind === "removable") : [];
  return candidates.length === 1 ? candidates[0] : null;
}

/** 一覧で目立たせるドライブか（カメラのカードか、取り外せるドライブ）。選ぶかどうかとは別で、広めに取る */
export function standsOut(drive: DriveLike): boolean {
  return drive.dcim_path !== null || drive.kind === "removable";
}
