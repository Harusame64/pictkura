/**
 * 試験と自動操作の手がかり（pictkura-dev #65）。画面に1つしか無い部品は `id="pk-…"`（UI Automation の
 * AutomationId になる）と `data-testid`、並ぶものは `data-testid` だけ。ここには作り方に決まりの要るものを置く
 */
/**
 * 「他のアプリで開く」の項目の id（pictkura-dev #65）。アプリの名前を英数字とハイフンにした形——並びの番号は
 * 台や一覧の順で別のアプリを指す。名前から作れない（英数字が無い）か、ほかの名前と重なるときだけ番号を添える
 */
export function openWithId(name: string, index: number, all: readonly string[]): string {
  const slug = (n: string) =>
    n
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
  const mine = slug(name);
  // 「other」は「他のアプリで開く…」（`open-with-other`）が使っているので、名前がそうなるときも番号を添える（PR の codex）
  const clash = mine === "other" || all.filter((n) => slug(n) === mine).length > 1;
  return mine && !clash ? `open-with-${mine}` : `open-with-${mine ? `${mine}-` : ""}${index}`;
}
