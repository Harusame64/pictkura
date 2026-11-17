import { useLayoutEffect, useReducer, useRef } from "react";
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
  day,
  url,
  label,
  loader,
  recent,
  waiters,
  onDone,
}: {
  /** 写真の id（空のタイルが、働き手の知らせを待つ宛先） */
  id: number;
  /** このタイルを並べている日（`day_key`）。撮影日が読まれて移った後に届く古い要求を、Rust が見分ける */
  day: number;
  url: string;
  label: string;
  loader: TileAttach;
  recent: RecentThumbs;
  waiters: BlankWaiters;
  /** 届いた・失敗した（`revealThumb`。地の灰色を見せる印を付ける） */
  onDone: (e: { currentTarget: HTMLImageElement }) => void;
}) {
  const ref = useRef<HTMLImageElement>(null);
  // 起こされたら下の effect を走り直させて問い直す（`BlankWaiters`）
  const [wakes, wakeUp] = useReducer((n: number) => n + 1, 0);
  // 一覧のタイルからの要求だと名乗る（`grid=1`）。サムネイルがまだ無い写真は、原本を渡さず空のタイルで
  // 待たせる（`src-tauri` の `requested_by_grid`）——出来れば URL（`thumb_state`）が変わり、この部品が読み直す。
  // それ以外の終わり方でも、働き手がこの写真を片付けたら問い直す（`BlankWaiters`）
  const src = `${url}&grid=1&day=${day}`;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const shown = recent.touch(src);
    if (shown !== undefined) {
      if (el.getAttribute("src") !== shown) el.src = shown;
      el.setAttribute("data-shown", "");
      return;
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
  }, [src, loader, recent, waiters, id, wakes]);
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
