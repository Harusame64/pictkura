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
export function letterOf(e: { key: string; code?: string }): string {
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (/^[a-z]$/.test(key)) return key;
  const m = /^Key([A-Z])$/.exec(e.code ?? "");
  return m ? m[1].toLowerCase() : key;
}
