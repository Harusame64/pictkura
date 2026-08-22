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

  // どの端末で見ているか。返すのは "win" / "mac" / "mac?" / "" のどれか。
  //
  // **Mac は CPU まで確かめられたときだけ "mac"** にする。配っているのは
  // Apple Silicon 版だけで、**Intel の Mac では動かない**のに、`navigator.platform`
  // はどちらも `MacIntel` を名乗るため——確かめずに arm64 へ直結すると、
  // 押した人に動かないものを渡すことになる（ゲート1の指摘）。CPU を読めるのは
  // Chromium 系の `getHighEntropyValues` だけなので、Safari など読めないブラウザは
  // "mac?" ＝ **区画は出すがボタンは選ぶ頁のまま**にしておく。区画の側に
  // 「Intel の Mac には対応していません」と書いてあるので、そこで気付ける。
  //
  // **iPadOS は "Macintosh" を名乗る**ので、触れる点の数で先に分ける。
  function detect() {
    var ua = navigator.userAgent || "";
    var data = navigator.userAgentData;
    var plat = (data && data.platform) || navigator.platform || "";

    if (/Android|iPhone|iPod|iPad/i.test(ua)) return Promise.resolve("");
    if (/Mac/i.test(plat) || /Mac OS X/i.test(ua)) {
      if (navigator.maxTouchPoints > 1) return Promise.resolve("");
      if (!data || !data.getHighEntropyValues) return Promise.resolve("mac?");
      return data.getHighEntropyValues(["architecture"]).then(function (v) {
        return v && v.architecture === "arm" ? "mac" : "mac?";
      }, function () {
        return "mac?";
      });
    }
    if (/Win/i.test(plat) || /Windows/i.test(ua)) return Promise.resolve("win");
    return Promise.resolve("");
  }

  // 端末に合った区画へ差し替える。**JS が無いときは Windows のぶんだけが出る**
  // （HTML 側で mac の区画に hidden を付けてある）。CPU を読めなかった Mac にも
  // 出すのは、Windows 版を勧めるよりは正しいため
  function swap(os) {
    if (os !== "mac" && os !== "mac?") return;
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

  // 押した時点で実物が落ちるボタン。**渡すものが確かなときだけ**書き換える
  // （"mac?" は含めない）。それ以外は埋めてある行き先＝選ぶ頁のまま
  function aim(assets, os) {
    var cta = document.querySelector("[data-pick-cta]");
    if (!cta || (os !== "win" && os !== "mac")) return;
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
  }

  // 端末の判定は通信と関係ないので、待たずに始めて先に区画を差し替える
  var seen = detect();
  seen.then(swap);

  fetch(API, { headers: { Accept: "application/vnd.github+json" } })
    .then(function (res) {
      if (!res.ok) throw new Error("HTTP " + res.status);
      return res.json();
    })
    .then(function (rel) {
      // 配布物の付いていない Release を掴んだら、書き換えずに埋めた版を残す
      if (!rel || !rel.assets || !rel.assets.length) throw new Error("no assets");
      return rel;
    })
    .catch(function () {
      // 引けなかった（回線、GitHub の回数制限、形の変更）。**埋めた版のまま**にして、
      // それが最新とは限らないことだけ断る。**ここで受けるのは取得の失敗だけ**——
      // 書き換えの側で転んだのを「取れなかった」と読ませないため
      each("[data-stale]", function (el) { el.hidden = false; });
      return null;
    })
    .then(function (rel) {
      if (!rel) return;
      apply(rel);
      return seen.then(function (os) { aim(rel.assets, os); });
    });
})();
