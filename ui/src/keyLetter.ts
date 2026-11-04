/**
 * キーボードの文字ショートカット（`P` / `X` / `Ctrl+K` …）を、**押したキーの文字**として読む。
 *
 * **`e.key` がラテン文字ならそれを使う**——AZERTY（仏）や QWERTZ（独）では、キーの刻印どおりの字で
 * 効くのが正しい（`A` の位置にある `Q` を押して `A` の操作が起きてはいけない）。
 *
 * **ラテン文字でないときだけ、物理位置（`e.code` の `KeyP` など）へ落とす**——露語の ЙЦУКЕН 配列では
 * `P` の位置で `з` が届き、`e.key` だけで照合するとビューアの ⚑ / ✕ / ★ も `Ctrl+K` も効かなかった
 * （2026-11-03、露語を足したときのゲート2）。ショートカットの一覧は `P` と案内しているので、
 * 配列を切り替えずに同じ位置を押せば効くようにする（Lightroom・Photoshop の露語版と同じ振る舞い）。
 */
export function letterOf(e: { key: string; code?: string; altKey?: boolean }): string {
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (/^[a-z]$/.test(key)) return key;
  // **落とすのはラテン文字でない字の配列だけ、かつ Alt 無しのとき**（codex）。拡張ラテン（ラトビア語の
  // AltGr で `ķ` など）は、ブラウザに Ctrl+Alt として届く。それを位置で `k` と読むと、字を打っただけで
  // `Ctrl+K` のパレットが開く。ラテン文字の配列は、どの字でも刻印どおりにしか読まない
  // 字は「文字」か「結合文字」（タイ語・ヒンディー語の配列は母音記号を出すキーがある）
  if (e.altKey || !/^[\p{L}\p{M}]$/u.test(key) || /^\p{Script=Latin}$/u.test(key)) return key;
  // `,` の位置も同じ（露語の配列ではそこが `б` で、一覧が案内する `Ctrl+,` が効かなかった。ゲート2）
  if (e.code === "Comma") return ",";
  const m = /^Key([A-Z])$/.exec(e.code ?? "");
  return m ? m[1].toLowerCase() : key;
}

/**
 * **数字のキー**（ビューアの 1 = 原寸・0 = 全体）。字を信じ、字が数字でないときだけ物理キーへ戻る:
 * - 字が数字なら、その数（ラテン・アラビア・インド・ペルシアの数字。テンキーも NumLock が入っていれば字が数字）
 * - 字が数字でなく、**修飾キーが1つも無い**ときだけ、数字の列の位置（`Digit1`）を見る——仏語の AZERTY は
 *   1 の位置で `&` を送る。US の Shift+1（`!`）・Alt+1・Ctrl+1 は拾わない
 * - テンキーの NumLock 切（`End` など）は字が数字でないので拾わない
 * Ctrl / ⌘ が押されていれば、字が数字でも拾わない（別のショートカットの領分）
 */
export function digitOf(e: {
  key: string;
  code?: string;
  altKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
}): number | null {
  if (e.ctrlKey || e.metaKey) return null;
  const m = /^[0-9٠-٩۰-۹]$/u.exec(e.key);
  if (m) {
    const c = e.key.charCodeAt(0);
    return c >= 0x06f0 ? c - 0x06f0 : c >= 0x0660 ? c - 0x0660 : c - 0x30;
  }
  if (e.altKey || e.shiftKey) return null;
  const d = /^Digit([0-9])$/.exec(e.code ?? "");
  return d ? Number(d[1]) : null;
}
