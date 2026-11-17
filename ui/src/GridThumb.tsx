import { useLayoutEffect, useRef, useState } from "react";
import { paintable, type BlankWaiters, type RecentThumbs } from "./gridThumbs";
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
 * そうでなければ列に並べ、読み終わったら入れる。URL が替わったら、新しい絵が届くまで前の絵を出したまま
 */
export function GridThumb({
  id,
  url,
  label,
  loader,
  recent,
  waiters,
  onDone,
}: {
  /** 写真の id（空のタイルが、働き手の知らせを待つ宛先） */
  id: number;
  url: string;
  label: string;
  loader: TileAttach;
  recent: RecentThumbs;
  waiters: BlankWaiters;
  /** 届いた・失敗した（`revealThumb`。地の灰色を見せる印を付ける） */
  onDone: (e: { currentTarget: HTMLImageElement }) => void;
}) {
  const ref = useRef<HTMLImageElement>(null);
  // 空のタイルが起こされた回数（いまの URL について。URL が替われば 0 から）。増えると下の effect が走り直して
  // 問い直す（`BlankWaiters`）
  const [woken, setWoken] = useState({ src: "", n: 0 });
  // 一覧のタイルからの要求だと名乗る（`grid=1`）。サムネイルがまだ無い写真は、原本を渡さず空のタイルで
  // 待たせる（`src-tauri` の `requested_by_grid`）——出来れば URL（`thumb_state`）が変わり、この部品が読み直す。
  // それ以外の終わり方でも、働き手がこの写真を片付けたら問い直す（`BlankWaiters`）
  const src = `${url}&grid=1`;
  const asks = woken.src === src ? woken.n : 0;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (recent.touch(src)) {
      if (el.getAttribute("src") !== src) el.src = src;
      el.setAttribute("data-shown", "");
      return;
    }
    // 問い直しは URL に回数を足す。同じ URL だと、macOS の WebView が同じ文書で読んだ空の絵をそのまま返しうる
    // （no-store でも。/code-review）。回数は働き手の知らせのたびに1つ増えるだけなので、URL が際限なく増えはしない
    const ask = asks === 0 ? src : `${src}&ask=${asks}`;
    // 知らせは読みを始める前から待つ——空の絵が届く前に知らせが来ると、届いてから待っても取りこぼす
    // （働き手の自動の回が、要求と入れ違いに素性だけ読んで終わったとき）。届く前の知らせは覚えておき、空が
    // 届いたら問い直す
    let blank = false;
    let woke = false;
    const askAgain = () => setWoken((w) => ({ src, n: (w.src === src ? w.n : 0) + 1 }));
    const wake = () => {
      if (blank) askAgain();
      else woke = true;
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
          recent.add(ask);
          el.src = ask;
        } else {
          // 出せる絵が無い（404・「サムネイルが無い」の 1x1）。前の URL の絵が残っていれば消し、地の灰色を
          // 見せる（古い向き・差し替え前の絵を出したままにしない）。覚えないので、作り直しで問い直す
          el.removeAttribute("src");
          el.setAttribute("data-shown", "");
          blank = true;
          // 届く前に知らせが来ていれば、すぐ問い直す（その知らせで待ちは外れている）。来ていなければ待ち続ける
          if (woke) askAgain();
        }
      },
      ask,
    );
    return () => {
      detach();
      unwait();
    };
  }, [src, loader, recent, waiters, id, asks]);
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
