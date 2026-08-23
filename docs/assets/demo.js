/* 動いている画面（`figure.demo` の中の video）を、いつ再生するか。
 *
 * **HTML には `autoplay` を書いていない。** 書いてしまうと、OSで「動きを減らす」を
 * 選んでいる人の画面でも、この JS が止めるまでの一瞬だけ動いてしまう（読み込みが
 * 速いほど確実に動く）。再生を始めるのは、この JS が設定を見たあとだけにする。
 *
 * つまり、こうなる:
 *
 *   動きを減らす設定  … 何もしない。操作の枠（controls）が出たまま止まっている
 *   ふつう            … 枠を消し、画面に入ったら再生・出たら停止する
 *   この JS が動かない … 操作の枠から自分で再生できる（HTML のままなので）
 */
(function () {
  "use strict";

  var vs = document.querySelectorAll(".demo video[data-autoplay]");
  if (!vs.length) return;

  // 動きを減らす設定なら、触らない。判定できない古い環境も同じ扱いにする
  if (!window.matchMedia || matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  // 画面の外の動画は再生しない。`autoplay` 属性でも Chrome は同じことをするが、
  // ここでは属性を使わないので自分で見る
  if (!window.IntersectionObserver) return;

  var io = new IntersectionObserver(function (entries) {
    for (var i = 0; i < entries.length; i++) {
      var v = entries[i].target;
      if (!entries[i].isIntersecting) { v.pause(); continue; }
      var p = v.play();
      // 自動再生が拒まれたら（端末の設定や省電力）、操作の枠を戻して人に任せる
      if (p && p.catch) p.catch(function (el) { return function () { el.controls = true; }; }(v));
    }
  }, { threshold: 0.25 });

  for (var i = 0; i < vs.length; i++) {
    vs[i].controls = false;
    io.observe(vs[i]);
  }
})();
