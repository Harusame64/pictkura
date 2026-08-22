/* 紹介サイトのダウンロード周り。
 *
 * **HTML に埋めてある版番号・リンク・大きさは、書いた時点のもの**。この JS が
 * GitHub の Releases を 1 回だけ引いて、最新の版へ書き換える。版を上げるたびに
 * サイトを直して回らずに済ませるためで、**JS が動かなくても埋めた版は落とせる**
 * （そのときは「最新とは限らない」という断りだけ出す）。
 *
 *   [data-file="末尾"] … その中の [data-dl] の行き先、[data-name]、[data-size] を差し替える
 *   [data-pick="win"|"mac"] … 見ている端末に合わせて出し分ける区画
 *   [data-pick-cta] … 押した時点で実物が落ちるボタン（紹介ページの見出し）
 *   [data-ver] [data-date] … 版と公開日
 *   [data-stale] … 引けなかったときだけ現れる断り
 *
 * 端末の判定は**外れても困らない**作りにしてある——外れたら選ぶ頁へ送るだけで、
 * ファイルの一覧はどの端末でも同じものが全部出る。
 */
(function () {
  "use strict";

  var API = "https://api.github.com/repos/Harusame64/pictkura/releases/latest";
  var RELEASES = "https://github.com/Harusame64/pictkura/releases";

  // 文言はほぼ HTML 側（data 属性）に置いてある。ここに残るのはこの 2 つだけ
  var ja = document.documentElement.lang === "ja";
  var SEP = ja ? "　・　" : " · ";
  var GONE = ja ? "この版にはありません" : "not in this release";

  function each(sel, fn) {
    Array.prototype.forEach.call(document.querySelectorAll(sel), fn);
  }

  function mb(bytes) {
    return (bytes / 1048576).toFixed(1) + " MB";
  }

  // 入れ物そのものが対象を兼ねることがある（一覧のタイルは <a> 自身が [data-dl]）
  function pick(box, sel) {
    var attr = sel.slice(1, -1);
    return box.hasAttribute(attr) ? box : box.querySelector(sel);
  }

  function put(box, sel, text) {
    var el = pick(box, sel);
    if (el) el.textContent = text;
  }

  // どの端末で見ているか。**iPadOS は "Macintosh" を名乗る**ので触れる点の数で分ける。
  // 分からなければ空を返す＝ボタンは選ぶ頁のままにする
  function detect() {
    var ua = navigator.userAgent || "";
    var plat = (navigator.userAgentData && navigator.userAgentData.platform) ||
               navigator.platform || "";
    if (/Android|iPhone|iPod|iPad/i.test(ua)) return "";
    if (/Mac/i.test(plat) || /Mac OS X/i.test(ua)) {
      return navigator.maxTouchPoints > 1 ? "" : "mac";
    }
    if (/Win/i.test(plat) || /Windows/i.test(ua)) return "win";
    return "";
  }

  var os = detect();

  // 端末に合った区画へ差し替える。**JS が無いときは Windows のぶんだけが出る**
  // （HTML 側で mac の区画に hidden を付けてある）。通信とは関係なく効かせたいので
  // 取得を待たずにここで済ませる
  function swap() {
    if (os !== "mac") return;
    var win = document.querySelector('[data-pick="win"]');
    var mac = document.querySelector('[data-pick="mac"]');
    if (win && mac) { win.hidden = true; mac.hidden = false; }
  }

  function find(assets, suffix) {
    if (!suffix) return null;
    for (var i = 0; i < assets.length; i++) {
      var name = assets[i].name || "";
      if (name.length >= suffix.length && name.slice(-suffix.length) === suffix) return assets[i];
    }
    return null;
  }

  // 押した時点で実物が落ちるボタン。**端末を判定できて、実物が見つかったときだけ**
  // 書き換える。それ以外は埋めてある行き先（選ぶ頁）のまま
  function aim(assets) {
    var cta = document.querySelector("[data-pick-cta]");
    if (!cta || !os) return;
    var hit = find(assets, cta.getAttribute("data-file-" + os));
    if (!hit) return;
    cta.href = hit.browser_download_url;
    var label = cta.getAttribute("data-cta-" + os);
    if (label) cta.textContent = label;
    each("[data-pick-meta]", function (el) {
      el.textContent = hit.name + SEP + mb(hit.size);
    });
  }

  function apply(rel) {
    var assets = rel.assets || [];

    each("[data-file]", function (box) {
      var hit = find(assets, box.getAttribute("data-file"));
      var link = pick(box, "[data-dl]");
      if (!hit) {
        // **無いものに古い版のリンクを残さない。** 配布物の名前や種類が変わったとき、
        // 押したら黙って前の版が落ちてくる、という経路を作らないため
        box.classList.add("dl-gone");
        if (link) link.href = rel.html_url || RELEASES;
        put(box, "[data-name]", "—");
        put(box, "[data-size]", GONE);
        return;
      }
      if (link) link.href = hit.browser_download_url;
      put(box, "[data-name]", hit.name);
      put(box, "[data-size]", mb(hit.size));
    });

    var ver = String(rel.tag_name || "").replace(/^v/, "");
    if (ver) each("[data-ver]", function (el) { el.textContent = ver; });
    if (rel.published_at) {
      each("[data-date]", function (el) { el.textContent = rel.published_at.slice(0, 10); });
    }

    aim(assets);
  }

  swap();

  fetch(API, { headers: { Accept: "application/vnd.github+json" } })
    .then(function (res) {
      if (!res.ok) throw new Error("HTTP " + res.status);
      return res.json();
    })
    .then(function (rel) {
      // 配布物の付いていない Release を掴んだら、書き換えずに埋めた版を残す
      if (!rel || !rel.assets || !rel.assets.length) throw new Error("no assets");
      apply(rel);
    })
    .catch(function () {
      // 引けなかった（回線、GitHub の回数制限、形の変更）。**埋めた版のまま**にして、
      // それが最新とは限らないことだけ断る
      each("[data-stale]", function (el) { el.hidden = false; });
    });
})();
