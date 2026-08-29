/* 上の帯のメニュー。
 *
 * 帯はこのサイトの目次なので中身は間引かない（site.css の `.bar` の注を見よ）。
 * ただし全部を横一列に並べると 1000px 台で折り返しが始まり、iPad の縦では帯が
 * 二段三段に伸びてしまう。そこで**狭い画面では並べ方だけを変える**——
 * 項目は ☰ の中へ畳み、帯にはブランドとダウンロードだけを残す。
 * 畳むのは見せ方で、行き先は 1 つも減らさない。
 *
 * head で（defer を付けずに）読んでいるのは、`js-nav` の印を最初の描画より前に
 * 立てるため。defer にすると一度フルのナビが出てから畳まれてちらつく。
 * JS が動かない端末ではこの印が付かず、帯は今まで通り折り返して全項目を出す。
 * つまり **JS 無しでも導線は 1 つも落ちない**。
 */
document.documentElement.classList.add("js-nav");

document.addEventListener("DOMContentLoaded", function () {
  var bar = document.querySelector(".bar");
  var btn = bar && bar.querySelector(".nav-toggle");
  var nav = bar && bar.querySelector(".nav");
  if (!btn || !nav) return;

  function isOpen() { return bar.hasAttribute("data-nav-open"); }
  function setOpen(open) {
    if (open) bar.setAttribute("data-nav-open", "");
    else bar.removeAttribute("data-nav-open");
    btn.setAttribute("aria-expanded", open ? "true" : "false");
  }

  btn.addEventListener("click", function () { setOpen(!isOpen()); });

  // 項目を押したら閉じる。同じ頁の中の錨（`#…`）へ飛ぶときも畳んでおかないと、
  // 開いたままの札が飛んだ先の見出しに被る
  nav.addEventListener("click", function (e) {
    if (e.target.closest("a")) setOpen(false);
  });

  // 外を押したら閉じる
  document.addEventListener("click", function (e) {
    if (isOpen() && !nav.contains(e.target) && !btn.contains(e.target)) setOpen(false);
  });

  // Escape で閉じ、焦点は開いたボタンへ戻す
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && isOpen()) { setOpen(false); btn.focus(); }
  });

  // 開いたまま横向きにするなどして幅が戻ったら、開いた印を捨てる。
  // 幅は site.css の断点と同じ値（片方だけ動かさないこと）
  var wide = window.matchMedia("(min-width: 1081px)");
  var onWide = function (e) { if (e.matches) setOpen(false); };
  if (wide.addEventListener) wide.addEventListener("change", onWide);
  else if (wide.addListener) wide.addListener(onWide);
});
