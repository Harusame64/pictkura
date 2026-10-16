/**
 * 写真を1枚だけ紙に載せて、OS の印刷へ渡す（ADR B-3）。
 *
 * 窓の中身は印刷のときだけ隠し、`<body>` の直下に置いた紙面（写真1枚）だけを刷る。紙面は**印刷の画面を
 * 閉じたら必ず片付ける**——残すと、次の印刷が前の写真になる（win の実測、2026-10-17）。
 *
 * 2つの OS で印刷の振る舞いが違う（spike の実測。`dev/adr.file-info-and-context-menu.md`）:
 *
 * - **Windows（WebView2）**: `window.print()` は**閉じるまで戻らない**。`@page { size }` が効くので、紙の向きを
 *   写真に合わせる。既定で日時・題・URL のヘッダーとフッターが付く——**`@page { margin: 0 }` で消える**
 * - **macOS（WKWebView）**: Tauri が `window.print` を自分の命令に差し替えていて、**Promise は画面が開いている間に
 *   解ける**。`@page { size }` は効かない（紙は既定の縦のまま）ので、横長の写真は紙面の上で90度回して載せる
 *
 * どちらも `afterprint` は画面を閉じたときに来る（キャンセルでも、保存でも）。
 */
/** 載せ方。CSS の `object-fit` の値そのもの（利用者の言葉では 拡大／クロップ／用紙いっぱい） */
export type PrintFit = "contain" | "cover" | "fill";

export const PRINT_FITS: readonly PrintFit[] = ["contain", "cover", "fill"];

const FIT_KEY = "pictkura.printFit";
const SHEET_ID = "print-sheet";
const STYLE_ID = "print-sheet-style";

/**
 * 写真を読んでいる間（HEIC は1秒ほど）。**重ねて押されたら受け付けない**——両方が紙面を足すと、macOS は
 * `print()` がすぐ戻るので2枚刷られ、Windows は印刷の画面が2回続けて出る（#213 のゲート2）
 */
let preparing = false;

/** 前に選んだ載せ方。読めない・知らない値なら「用紙に合わせて拡大」 */
export function loadPrintFit(): PrintFit {
  try {
    const v = localStorage.getItem(FIT_KEY);
    if (v && (PRINT_FITS as readonly string[]).includes(v)) return v as PrintFit;
  } catch {
    // 保存先が使えない窓でも印刷はできる
  }
  return "contain";
}

export function savePrintFit(fit: PrintFit): void {
  try {
    localStorage.setItem(FIT_KEY, fit);
  } catch {
    // 覚えられなくても、今回の印刷には効いている
  }
}

/** 紙面と、それを出すための CSS を外す。何度呼んでもよい */
function clearSheet(): void {
  document.getElementById(SHEET_ID)?.remove();
  document.getElementById(STYLE_ID)?.remove();
}

/**
 * 紙面の CSS。`landscape` は写真が横長か。
 *
 * 紙の向きが写真と逆になったとき（macOS は `@page { size }` を聞かないので必ずこうなる。Windows でも利用者が
 * 印刷の画面で向きを変えればなる）は、写真を90度回して紙の長い辺に合わせる。
 *
 * **紙の大きさは紙面（コンテナ）で測る。** WebKit の印刷では `vw`・`vh` と `@media (orientation)` が紙ではなく
 * **窓**の大きさで決まり、写真が回らず左上に寄って切れた（mac の実測、2026-10-17）。紙面を `height: 100%` で
 * 紙に合わせ、`@container (orientation)` と `cqw`・`cqh` で読む
 */
export function sheetCss(fit: PrintFit, landscape: boolean): string {
  const other = landscape ? "portrait" : "landscape";
  return `
@page { size: ${landscape ? "landscape" : "portrait"}; margin: 0; }
#${SHEET_ID} { display: none; }
@media print {
  html, body {
    margin: 0 !important;
    padding: 0 !important;
    height: 100% !important;
    overflow: hidden !important;
    background: #fff !important;
  }
  body > *:not(#${SHEET_ID}) { display: none !important; }
  #${SHEET_ID} {
    display: block;
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    break-inside: avoid;
    container-type: size;
  }
  #${SHEET_ID} img {
    position: absolute;
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
    object-fit: ${fit};
  }
}
@container (orientation: ${other}) {
  #${SHEET_ID} img {
    left: 50%;
    top: 50%;
    width: 100cqh;
    height: 100cqw;
    transform: translate(-50%, -50%) rotate(90deg);
  }
}`;
}

/**
 * `src` の写真を `fit` で載せて印刷の画面を出す。写真を読めなければ辞書の鍵 `errPrint` を名に持つ Error を投げる
 * （紙面は残さない。`errText` へは `message` を渡す）。
 *
 * 返るのは画面を出したところまで（Windows は閉じたところまで）。片付けは `afterprint` で行う
 */
export async function printPhoto(src: string, fit: PrintFit): Promise<void> {
  if (preparing) return;
  // 前の紙面が残っていたら（`afterprint` が来なかった等）、ここで外す——紙面が2つあると2枚刷られる
  clearSheet();
  const img = new Image();
  img.decoding = "sync";
  img.src = src;
  preparing = true;
  try {
    await img.decode();
  } catch {
    throw new Error("errPrint");
  } finally {
    preparing = false;
  }
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = sheetCss(fit, img.naturalWidth > img.naturalHeight);
  const sheet = document.createElement("div");
  sheet.id = SHEET_ID;
  sheet.appendChild(img);
  document.head.appendChild(style);
  document.body.appendChild(sheet);
  window.addEventListener("afterprint", clearSheet, { once: true });
  try {
    // macOS は Promise（Tauri の差し替え）、Windows は undefined（閉じるまで戻らない）
    await (window.print as () => unknown)();
  } catch (e) {
    // macOS で命令が拒まれた等。画面は出ていないので `afterprint` は来ない
    window.removeEventListener("afterprint", clearSheet);
    clearSheet();
    throw e;
  }
}
