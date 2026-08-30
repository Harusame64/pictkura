/**
 * このOSを1回だけ聞いて配る。
 *
 * **正はバックエンドの `cfg!`**（`host_platform`）。`api.ts` の `isMac` /
 * `isWindows` は `navigator.userAgent` を見るもので、修飾キーの表記を選ぶには
 * 十分でも、**WebViewのUA次第で外れる**。「買わせる話をするか」「デコーダは
 * あると断言するか」「AutoPlayの設定を出すか」を取り違えると実害が出る。
 *
 * **問い合わせは1回だけ。** 使う側が増えるたびにIPCを増やすと、
 * 画面ごとに答えが食い違いうる（設定を開いた瞬間にもう1回聞いていた）。
 * 約束をモジュールに1つ持ち、全員がそれを待つ。
 *
 * 届くまではUAの推測を返す。`getHostPlatform` が転んでも推測のまま
 * ——**判定が落ちて機能ごと消えるより、当たる確率の高い推測で出しておく**。
 */
import { useEffect, useState } from "react";

import { getHostPlatform, isMac, isWindows, type HostPlatform } from "./api";

/** UAからの当て推量（届くまでの繋ぎ） */
const guess: HostPlatform = isWindows ? "windows" : isMac ? "macos" : "other";

/**
 * 今いちばん確かな答え。**届いたら推測を捨てて置き換える**。
 *
 * 後から開く画面（設定ダイアログ等）が、**もう答えが出ているのに推測から
 * 描き始める**のを防ぐ。1フレームだけ違う値で描くと、AutoPlayの節のように
 * 出す・出さないが変わるものは**一瞬ちらつく**（ゲート2の指摘）
 */
let known: HostPlatform = guess;

/**
 * **バックエンドが答えた**か（推測のあいだは偽）。
 *
 * 推測で分岐してよいかは**外したときの代償が対称かどうか**で決まる。
 * 修飾キーの表記なら一瞬違っても直るが、[`useConfirmedPlatform`] を使う側は
 * そうではない
 */
let answered = false;

/** 起動中に1回だけ走る問い合わせ。全員がこの約束を共有する */
const resolved: Promise<HostPlatform> = getHostPlatform()
  .then((p) => {
    known = p;
    answered = true;
    return p;
  })
  .catch(() => guess);

export function usePlatform(): HostPlatform {
  const [platform, setPlatform] = useState<HostPlatform>(known);
  useEffect(() => {
    let alive = true;
    resolved.then((p) => {
      if (alive) setPlatform(p);
    });
    return () => {
      alive = false;
    };
  }, []);
  return platform;
}

/**
 * **バックエンドが答えたOSだけ**を返す。届くまで（＝推測のあいだ）は `null`。
 *
 * [`usePlatform`] との違いは**外し方**である。あちらは「判定が落ちて機能ごと
 * 消えるより、当たる確率の高い推測で出す」側に倒してある。こちらは逆に倒す
 * ——**外したときの代償が非対称なところ**で使う。
 *
 * いまの利用者は `App.tsx` の先読みの触り直しで、**windows側へ外すとWebKitで
 * 1ティックにつき24MP1枚ぶんが漏れる**のに対し、逆へ外しても失うのは
 * 送り100msぶんの最適化だけ。だから**答えが来るまで動かさない**。
 * 問い合わせ自体が転んだときも `null` のまま——安全な側に倒れる。
 */
export function useConfirmedPlatform(): HostPlatform | null {
  const [platform, setPlatform] = useState<HostPlatform | null>(
    answered ? known : null,
  );
  useEffect(() => {
    let alive = true;
    void resolved.then((p) => {
      // `catch` で推測に落ちた回は `answered` が立たない＝ここでも出さない
      if (alive && answered) setPlatform(p);
    });
    return () => {
      alive = false;
    };
  }, []);
  return platform;
}
