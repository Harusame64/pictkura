import { useLayoutEffect, useReducer, useRef, useState } from "react";
import { paintable, railMotion, type BlankWaiters, type RecentThumbs } from "./gridThumbs";
import type { TileAttach } from "./useTileQueue";

/** タイルごとの鍵（同じ写真のタイルが同時に2枚ありうるので、URL とは別に振る） */
let seq = 0;

/**
 * 一覧のタイルの絵。`src` はこの部品が入れる（`gridThumbs.ts` の説明）。
 *
 * 付け外しは React の effect に任せる——ref の呼び出しで自前に数えると、React 18 は要素を文書から外す前に
 * ref に null を渡すので外したタイルを見分けられず、列に残り続けた（/code-review）。effect なら、外れたときと
 * URL が替わったときに、そのタイルの片付けが必ず走る。
 *
 * 描く前（layout effect）に決める: 最近出した絵ならその場で `src` と印を入れ、浮かび直させない。
 * サムネイルがもうある写真（`ready`）は、列を通さずすぐ `src` を入れる。まだ無い写真は列に並べ（Rust は原本を
 * 返しつつサムネイルを作る列へ積む）、読み終わったら入れる。URL が替わったら、新しい絵が届くまで前の絵を出したまま
 */
export function GridThumb({
  id,
  day,
  url,
  ready,
  quick,
  label,
  loader,
  recent,
  waiters,
  onDone,
}: {
  /** 写真の id（空のタイルが、働き手の知らせを待つ宛先） */
  id: number;
  /**
   * このタイルを並べている日（`day_key`）。撮影日が読まれて移った後に届く古い要求を、Rust が見分けて作らない
   * （新しいライブラリで、画面から去った写真の本番を作らない）。検索中は渡さない——差し替えが日を動かさない
   */
  day?: number;
  url: string;
  /**
   * サムネイル（即席でも本番でも）がもうある。**列を通さず、すぐ `src` に入れる**——DOM に入った瞬間から読み始め、
   * 画面に入る前に読み終わる。列（同時4枚・見えているものから）は原本を読むときだけでよい。小さな絵を列で
   * 絞ると、一気に送った間、画面に入ってから読み始めて間に合わなかった（win2 の画素の計器: 初めて通る所の
   * 写真の割合 0.40 → 0.80〜0.89。即席だけのライブラリでも、全部本番を作ったライブラリでも同じ）
   */
  ready?: boolean;
  /** あるのは即席だけ（本番はまだ）。見えている間は本番を作る列に積まれるので、去ったら降ろす */
  quick?: boolean;
  label: string;
  loader: TileAttach;
  recent: RecentThumbs;
  waiters: BlankWaiters;
  /** 届いた・失敗した（`revealThumb`。地の灰色を見せる印を付ける） */
  onDone: (e: { currentTarget: HTMLImageElement }) => void;
}) {
  const ref = useRef<HTMLImageElement>(null);
  // `ready` の URL が空・404 で返った（一覧の行が古く、サムネイルが消えていた等）。その URL の間だけ列の道で問い直す
  // （URL が替われば元へ戻す）
  const [staleSrc, setStaleSrc] = useState<string | null>(null);
  // 起こされたら下の effect を走り直させて問い直す（`BlankWaiters`）
  const [wakes, wakeUp] = useReducer((n: number) => n + 1, 0);
  // 一覧のタイルからの要求だと名乗る（`grid=1`）。サムネイルがまだ無い写真は、Rust が作る列の先頭へ押し、
  // 描ける原本（寸法が分かっていて上限以内）なら原本を、描けなければ空のタイルを返す（`src-tauri` の
  // `requested_by_grid`）——出来れば URL（`thumb_state`）が変わり、この部品が読み直す。
  // それ以外の終わり方でも、働き手がこの写真を片付けたら問い直す（`BlankWaiters`）
  const src = day === undefined ? `${url}&grid=1` : `${url}&grid=1&day=${day}`;
  const stale = staleSrc === src;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const shown = recent.touch(src);
    if (shown !== undefined) {
      if (el.getAttribute("src") !== shown) el.src = shown;
      el.setAttribute("data-shown", "");
      return;
    }
    el.removeAttribute("data-instant");
    // 帯を掴んで動かしている間は、サムネイルがあっても列（同時4枚・見えているものから・去れば降ろす）を通す
    // （`railMotion`。直接読むと通り過ぎる絵の展開と描画が溜まり、止めた後 100〜400ms 灰色で固まった。#80）
    if (ready && !stale && !railMotion.dragging) {
      // 出すのは届いてから（`onDone`）。先に印を付けると、届くまでタイルの地（明るい灰色）が見えた。届いたら
      // フェードせずにその場で出す（`data-instant`。一気送りではフェードの途中の暗い絵のまま去った）。
      // 覚えるのも、出せる絵が届いてから——空や 404 なら覚えず、列の道で問い直す（行が古くサムネイルが消えて
      // いたとき、空を覚えて灰色のまま残った。PR #356 の /code-review）
      el.setAttribute("data-instant", "");
      const settle = () => {
        if (paintable(el)) {
          recent.add(src, src);
        } else {
          el.removeAttribute("src");
          setStaleSrc(src);
        }
      };
      el.addEventListener("load", settle, { once: true });
      el.addEventListener("error", settle, { once: true });
      el.src = src;
      return () => {
        el.removeEventListener("load", settle);
        el.removeEventListener("error", settle);
      };
    }
    // 知らせが来ていれば、その数を URL に足して問い直す（同じ URL は macOS で前の空の絵が返りうる。`BlankWaiters`）
    const asked = waiters.count(id);
    const ask = asked === 0 ? src : `${src}&ask=${asked}`;
    // 知らせは読みを始める前から待つ。空の絵が届く前に来た知らせは、届いたときに数が進んでいることで分かる
    let blank = false;
    const wake = () => {
      if (blank) wakeUp();
    };
    const unwait = waiters.wait(id, wake);
    const detach = loader.attach(
      `${ask}#${++seq}`,
      el,
      (img) => {
        // 読み終わったら列から外す（見張りの知らせを受け続けない）
        detach();
        if (paintable(img)) {
          unwait();
          recent.add(src, ask);
          el.src = ask;
        } else {
          // 出せる絵が無い（404・「サムネイルが無い」の 1x1）。前の URL の絵が残っていれば消し、地の灰色を
          // 見せる（古い向き・差し替え前の絵を出したままにしない）。覚えないので、作り直しで問い直す
          el.removeAttribute("src");
          el.setAttribute("data-shown", "");
          blank = true;
          // 届く前に知らせが来ていれば、すぐ問い直す（その知らせで待ちは外れている）。来ていなければ待ち続ける
          if (waiters.count(id) !== asked) wakeUp();
        }
      },
      ask,
    );
    return () => {
      detach();
      unwait();
    };
  }, [src, loader, recent, waiters, id, wakes, ready, stale]);
  // **画面から去ったとき**（URL が替わったときではない——そのときは同じタイルがすぐ頼み直す）、サムネイルが
  // まだ無ければ作る列から降ろす（`BlankWaiters.leave`）。原本が描けたタイルも降ろす——Rust は原本を返すときも
  // 列に積んでいて、通り過ぎた写真の本番が、止めた画面の HEIC・RAW の前に並んだ（PR #356 の /code-review）
  // 即席だけのタイルも降ろす——見えている間に `set_visible_priority` が本番を積んでいる。本番があるタイルは列に
  // 居ないので送らない（一気送りで何百枚ぶんの空振りの往復になった。PR #356 の /code-review）
  const leaveRef = useRef(false);
  leaveRef.current = !ready || !!quick || stale;
  useLayoutEffect(
    () => () => {
      if (leaveRef.current) waiters.leave(id);
    },
    [waiters, id],
  );
  return (
    <img
      ref={ref}
      className="cell"
      decoding="async"
      // 届いたら浮かべる（`revealThumb`）。失敗でも浮かべる——背景の灰色が「サムネイルが無い」の見た目なので、隠したままにしない
      onLoad={onDone}
      onError={onDone}
      // `alt` 未指定だと Chromium は title を代替テキストとして枠に描いてしまう（ファイル名が並ぶ）。装飾用として空にする
      alt=""
      // **絵をボタンとして、ファイル名で名乗る**（UI Automation では名前の無い Group だった。pictkura-dev #65）。
      // タイル全体をボタンにすると、中の「選択」や重ねの印のボタンが読み上げから潰れる（ゲート2）。
      // 押すとタイルの onClick へ上がる。Tab では止めない——一覧には自分のキー操作がある
      role="button"
      aria-label={label}
    />
  );
}
