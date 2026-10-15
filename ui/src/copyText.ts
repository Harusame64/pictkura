/**
 * 文字列をクリップボードへ写す（pictkura-dev #40: 送り出しフォルダのパスをコピー）。
 *
 * `navigator.clipboard` を先に使う。WebView によっては使えない・断られるので、そのときは
 * 隠したテキスト欄を選んで `execCommand("copy")` で写す。どちらも駄目なら false（画面はパスを見せたままなので、手で選べる）
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 下の道で試す */
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}
