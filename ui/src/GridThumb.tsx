import { useLayoutEffect, useRef } from "react";
import { paintable, type RecentThumbs } from "./gridThumbs";
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
  url,
  label,
  loader,
  recent,
  onDone,
}: {
  url: string;
  label: string;
  loader: TileAttach;
  recent: RecentThumbs;
  /** 届いた・失敗した（`revealThumb`。地の灰色を見せる印を付ける） */
  onDone: (e: { currentTarget: HTMLImageElement }) => void;
}) {
  const ref = useRef<HTMLImageElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (recent.touch(url)) {
      if (el.getAttribute("src") !== url) el.src = url;
      el.setAttribute("data-shown", "");
      return;
    }
    const detach = loader.attach(
      `${url}#${++seq}`,
      el,
      (img) => {
        // 読み終わったら列から外す（見張りの知らせを受け続けない）
        detach();
        if (paintable(img)) {
          recent.add(url);
          el.src = url;
        } else {
          // 出せる絵が無い（404・「サムネイルが無い」の 1x1）。前の URL の絵が残っていれば消し、地の灰色を
          // 見せる（古い向き・差し替え前の絵を出したままにしない）。覚えないので、作り直しで問い直す
          el.removeAttribute("src");
          el.setAttribute("data-shown", "");
        }
      },
      url,
    );
    return detach;
  }, [url, loader, recent]);
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
