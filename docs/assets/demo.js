/* 動いている画面（`figure.demo` の中の video）の面倒だけを見る。
 *
 * OS で「動きを減らす」を選んでいる人には、**勝手に動き出さない**ようにする。
 * HTML の `autoplay` は CSS からは止められないので、ここで外して操作の枠を出す。
 * この JS が動かなくても、動画はそのまま自動再生されるだけで何も壊れない。
 */
(function () {
  "use strict";

  if (!window.matchMedia) return;
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  var vs = document.querySelectorAll(".demo video");
  for (var i = 0; i < vs.length; i++) {
    var v = vs[i];
    v.autoplay = false;
    v.loop = false;
    v.controls = true;
    v.pause();           // defer で走るので、もう再生が始まっていることがある
  }
})();
