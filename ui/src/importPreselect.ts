/**
 * 取り込みの画面を開いたときに、どのドライブを最初から選んでおくか（pictkura-dev #39）の**判定だけ**。
 *
 * 利用者の声（2026-10-10）:「開いた時点でリムーバブルドライブ（特にSDカード）を選択状態とするか、
 * もう少し存在感を目立たせた方が良い」。**候補がちょうど1つのときだけ選び、複数なら目立たせるだけ**にする。
 *
 * **候補は「取り外せる、かつ `DCIM` を持つドライブ」を先に見る**（カメラのカード）。
 * - macOS では外付けのバックアップ用ディスクも `removable` と申告され、SD カードと見分けが付かない
 *   （`list_drives` の注）。`DCIM` を持たないものは選ばない——開いただけでバックアップ用ディスクを丸ごと走査しない
 * - `DCIM` を持っていても**固定ディスクは選ばない**（ゲート2）。外付け HDD に置いたカメラの控え（`X:\DCIM`）を
 *   開いた瞬間に走査し出すのは、固定ドライブの直下は自動で走査しない、というこの画面の約束に反する
 * - `DCIM` を持つものが無いときだけ、**Windows の取り外せるドライブ**（`GetDriveTypeW` の判定。USB メモリ）を候補にする。
 *   カード読み取り機の空のスロットは容量 0 で `list_drives` が外すので、ここには来ない
 *
 * **開く場所は `DCIM` から**（ゲート2）。ツリーのクリックも AutoPlay も `DCIM` に絞っている——一番上から走査すると、
 * 動画のサムネイル（`/PRIVATE/M4ROOT/THMBNL`）のような写真でない画像まで「未取り込み」として選ぶ。
 *
 * React も `window` も持ち込まない（`scripts/importPreselect.test.ts` から `node --test` で読む。型だけ `api.ts` から）。
 */
import type { DriveInfo, HostPlatform } from "./api.ts";

/** 判定に要るドライブの形 */
export type DriveLike = Pick<DriveInfo, "path" | "kind" | "dcim_path">;

/** 最初から選んでおくドライブ。候補がちょうど1つのときだけ返す。無ければ・複数なら null */
export function preselectDrive<D extends DriveLike>(drives: readonly D[], platform: HostPlatform): D | null {
  const removable = drives.filter((d) => d.kind === "removable");
  const cards = removable.filter((d) => d.dcim_path !== null);
  const candidates = cards.length > 0 ? cards : platform === "windows" ? removable : [];
  return candidates.length === 1 ? candidates[0] : null;
}

/** 選んだドライブで最初に開く場所（カードなら `DCIM`） */
export function openPathOf(drive: DriveLike): string {
  return drive.dcim_path ?? drive.path;
}

/**
 * いま見ているフォルダが、そのドライブの中か（区切りまで見る。`/Volumes/SD` は `/Volumes/SD2` を含まない）。
 * 中なら、開き直しても選び直さない——利用者がカードの中の別のフォルダを見ていたら、そのままにする
 */
export function isViewing(view: string | null, drive: DriveLike): boolean {
  if (!view) return false;
  if (view === drive.path) return true;
  return view.startsWith(withSeparator(drive.path));
}

/** 末尾に区切りを付けた形（`E:\` はそのまま、`/Volumes/SD` は `/Volumes/SD/`） */
function withSeparator(path: string): string {
  return /[\\/]$/.test(path) ? path : path + (path.includes("\\") ? "\\" : "/");
}

/**
 * ツリーで `path` の行を見せるために開く枝（根から `path` の親まで）。どの根の中でもなければ空。
 *
 * どの入口（ドライブの行・AutoPlay・最初から選ぶカード）も `X:\DCIM` を開くが、ツリーの根はドライブなので、
 * 根を開かないと `DCIM` の行が出ず、選んでいる行がどこにも見えない（#203 の win の実機）。
 * 根が入れ子なら（ドライブの中に自分で足したフォルダ）、深いほうから辿る。
 */
export function branchTo(roots: readonly string[], path: string): string[] {
  // 自分で足したフォルダはそれ自体が根の行なので、上のドライブ（`C:\`・`/`）を開いて辿らない
  if (roots.includes(path)) return [];
  const root = roots
    .filter((r) => path.startsWith(withSeparator(r)))
    .reduce<string | null>((a, r) => (a === null || r.length > a.length ? r : a), null);
  if (root === null) return [];
  const base = withSeparator(root);
  const sep = base.endsWith("\\") ? "\\" : "/";
  const parts = path.slice(base.length).split(/[\\/]/).filter((x) => x !== "");
  const branch = [root];
  for (let i = 1; i < parts.length; i++) branch.push(base + parts.slice(0, i).join(sep));
  return branch;
}

/** 一覧で目立たせるドライブか（カメラのカードか、取り外せるドライブ）。選ぶかどうかとは別で、広めに取る */
export function standsOut(drive: DriveLike): boolean {
  return drive.dcim_path !== null || drive.kind === "removable";
}
