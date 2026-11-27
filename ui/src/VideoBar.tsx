import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Icon } from "./Icon";
import { t } from "./i18n";
import { num } from "./i18n/plural";
import { SKIP_SECONDS, SPEEDS, canSeek, formatSpeed, formatTime, seekTarget } from "./videoControls";

/**
 * ビューアの動画の再生バー（自作。dev #58、dev/plan.video-controls.md）。
 *
 * 標準の `<video controls>` は、Windows（WebView2）で音量のつまみ・シークバーに焦点があると ← → と Esc を頁に
 * 一切渡さず、マウスの pointer/mouse/click も渡さなかった（届くのは focusin だけ。win の実機 2 周、PR #376）。
 * 送りも全画面を抜けるのも効かなくなるので、バーをこちらで描き、焦点を持たせない。
 *
 * - 載せる物は標準と同じだけ（利用者の選択）: 再生・10 秒戻す/送る・シークと時間・ミュートと音量・再生速度・
 *   ピクチャー イン ピクチャー・全画面（アプリの全画面へ）。ダウンロードは載せない（ビューアの保存がある）
 * - **マウスで触っても焦点を残さない**——押して離したら、ビューアの外枠へ返す（`onReturnFocus`）。離すのが
 *   バーの外のこともある（つまみを端の先まで引く）ので、押したときに window で離すのを待つ。
 *   Tab で来た人はそのまま（つまみの ← → で動かせる。App.tsx のキーの受け手はつまみの矢印を横取りしない）
 * - 見た目は YouTube の形（利用者 11-27）: 動画の幅いっぱい、下から暗くなる帯の上に、上の段が再生位置、下の段が
 *   左の塊（再生・10 秒・音量・時間）と右の塊（速度・PiP・全画面）。マウスを止めると他の道具と一緒に消える
 *   （App.css の idle の規則）
 */
export function VideoBar({
  video,
  fullscreen,
  onFullscreen,
  onReturnFocus,
}: {
  video: HTMLVideoElement | null;
  fullscreen: boolean;
  onFullscreen: () => void;
  onReturnFocus: () => void;
}) {
  const [paused, setPaused] = useState(true);
  const [current, setCurrent] = useState(0);
  const [duration, setDuration] = useState(NaN);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [rate, setRate] = useState(1);
  const [pip, setPip] = useState(false);
  const [speedOpen, setSpeedOpen] = useState(false);
  // つまみを引いている間は、再生の進み（timeupdate）でつまみを引き戻さない
  const scrubbing = useRef(false);
  // 速度のボタンと一覧を包む箱。外を押したかは、印ではなく押した先がこの中かで決める——印は、バーが押下の
  // 伝わりを止めると消されずに残り、次の外の押下を見送った（#378 の codex）
  const speedWrap = useRef<HTMLSpanElement>(null);
  // バーそのもの。一覧を開いたままバーの別の部品（再生・つまみ）を押したら、一覧を閉じてその部品も効かせる
  const barRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // 次の動画へ移ったら、開いていた速度の一覧とつまみの引きは持ち越さない
    setSpeedOpen(false);
    scrubbing.current = false;
    if (!video) return;
    const sync = () => {
      setPaused(video.paused);
      if (!scrubbing.current) setCurrent(video.currentTime);
      setDuration(video.duration);
      setVolume(video.volume);
      setMuted(video.muted);
      setRate(video.playbackRate);
      setPip(document.pictureInPictureElement === video);
    };
    sync();
    const events = [
      "play",
      "pause",
      "timeupdate",
      "durationchange",
      "loadedmetadata",
      "volumechange",
      "ratechange",
      "seeked",
      "enterpictureinpicture",
      "leavepictureinpicture",
    ];
    for (const ev of events) video.addEventListener(ev, sync);
    return () => {
      for (const ev of events) video.removeEventListener(ev, sync);
    };
  }, [video]);

  // 速度の一覧は、外を押すか Esc で閉じる（Esc はビューアを閉じる前にここが取る）
  useEffect(() => {
    if (!speedOpen) return;
    const close = (e: PointerEvent) => {
      if (e.target instanceof Node && speedWrap.current?.contains(e.target)) return;
      // 手前に幕（パレット・設定）が開いてビューアが inert なら、押下はその幕のもの。ここで閉じてクリックを
      // 飲むと、幕のボタンの 1 回目が効かなかった（#378 の codex。Esc と同じ決まり）
      if (speedWrap.current?.closest("[inert]")) return;
      setSpeedOpen(false);
      // バーの中の別の部品なら、閉じてそのまま効かせる（再生を 2 回押させない。つまみはそこへ動く。#378 の codex）
      if (e.target instanceof Node && barRef.current?.contains(e.target)) return;
      // バーの外を押したら、一覧を閉じるだけにする——その押下のクリックは地にも動画にも届けない。届けると、地なら
      // ビューアまで閉じ、動画なら再生が始まった（#378 の win の実機）。押して離した後のクリック 1 回だけを受け止める
      const eat = (e: MouseEvent) => {
        e.stopPropagation();
        e.preventDefault();
      };
      const done = () => {
        window.removeEventListener("pointerup", done, true);
        window.removeEventListener("pointercancel", done, true);
        // クリックは pointerup の後に来る。その後で外す（ドラッグで終わってクリックが来なくても残らない）
        setTimeout(() => window.removeEventListener("click", eat, true), 0);
      };
      window.addEventListener("click", eat, true);
      window.addEventListener("pointerup", done, true);
      window.addEventListener("pointercancel", done, true);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // ビューアの上に別の幕（パレット・設定）が開いて、ビューアが inert になっているなら、Esc はその幕のもの
      // （#378 の codex: 奥の一覧が先に取り、手前の幕が 1 回目の Esc で閉じなかった）
      if (speedWrap.current?.closest("[inert]")) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      setSpeedOpen(false);
    };
    // capture で見る——バーの押下は伝わりを止める（地へ届けない）ので、bubble では外か中かを見られない
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [speedOpen]);

  if (!video) return null;

  const seekable = canSeek(duration);
  const silent = muted || volume === 0;
  const pipAvailable = typeof document !== "undefined" && document.pictureInPictureEnabled === true;

  // 押したら、離したところで焦点をビューアへ返す（このバーは自分の DOM なので、標準のバーと違って届く）
  const holdFocusUntilRelease = () => {
    const giveBack = () => {
      window.removeEventListener("pointerup", giveBack, true);
      window.removeEventListener("pointercancel", giveBack, true);
      scrubbing.current = false;
      requestAnimationFrame(onReturnFocus);
    };
    window.addEventListener("pointerup", giveBack, true);
    window.addEventListener("pointercancel", giveBack, true);
  };

  return (
    <div
      ref={barRef}
      className="viewer-vbar"
      dir="ltr"
      // 地のクリック（ビューアを閉じる）・右クリックのメニュー・送りの動きへ届けない
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.stopPropagation()}
      // 押下は止めない——window の「動いたら道具を出し直す」受け口（App.tsx の wakePointer）まで届かないと、
      // 速度の一覧を開いて手を止めた間にバーごと消えた（#378 の win の実機）。地へ届けないのはクリックの方（上）
      onPointerDown={holdFocusUntilRelease}
    >
      {/* 再生位置は上の段に、動画の幅いっぱい（YouTube の形。利用者 11-27「細くて小さい」）。長さの分からない動画
          （MediaRecorder の webm など）はシークできないので、標準のバーと同じく帯を出さない */}
      {seekable && (
        <input
          className="vbar-seek"
          type="range"
          min={0}
          max={duration}
          step="any"
          value={Math.min(current, duration)}
          style={{ "--p": `${(Math.min(current, duration) / duration) * 100}%` } as CSSProperties}
          aria-label={t.videoSeek}
          aria-valuetext={`${formatTime(current)} / ${formatTime(duration)}`}
          onPointerDown={() => {
            scrubbing.current = true;
          }}
          onChange={(e) => {
            const v = Number(e.currentTarget.value);
            setCurrent(v);
            video.currentTime = v;
          }}
        />
      )}
      <div className="vbar-row">
        <div className="vbar-group">
          <button
            className="vbar-btn"
            title={paused ? t.videoPlay : t.videoPause}
            aria-label={paused ? t.videoPlay : t.videoPause}
            onClick={() => {
              if (video.paused) void video.play().catch(() => {});
              else video.pause();
            }}
          >
            <Icon name={paused ? "play" : "pause"} size={24} />
          </button>
          <button
            className="vbar-btn"
            title={t.videoBack(SKIP_SECONDS)}
            aria-label={t.videoBack(SKIP_SECONDS)}
            disabled={!seekable}
            onClick={() => {
              video.currentTime = seekTarget(video.currentTime, -SKIP_SECONDS, video.duration);
            }}
          >
            <Icon name="skipBack" size={24} />
          </button>
          <button
            className="vbar-btn"
            title={t.videoForward(SKIP_SECONDS)}
            aria-label={t.videoForward(SKIP_SECONDS)}
            disabled={!seekable}
            onClick={() => {
              video.currentTime = seekTarget(video.currentTime, SKIP_SECONDS, video.duration);
            }}
          >
            <Icon name="skipForward" size={24} />
          </button>
          <button
            className="vbar-btn"
            title={silent ? t.videoUnmute : t.videoMute}
            aria-label={silent ? t.videoUnmute : t.videoMute}
            aria-pressed={silent}
            onClick={() => {
              if (silent) {
                video.muted = false;
                if (video.volume === 0) video.volume = 0.5;
              } else {
                video.muted = true;
              }
            }}
          >
            <Icon name={silent ? "volumeMute" : "volume"} size={24} />
          </button>
          <input
            className="vbar-volume"
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={muted ? 0 : volume}
            style={{ "--p": `${(muted ? 0 : volume) * 100}%` } as CSSProperties}
            aria-label={t.videoVolume}
            aria-valuetext={`${Math.round((muted ? 0 : volume) * 100)}%`}
            onChange={(e) => {
              const v = Number(e.currentTarget.value);
              video.volume = v;
              video.muted = v === 0;
            }}
          />
          <span className="vbar-time">
            {formatTime(current)}
            {seekable && <> / {formatTime(duration)}</>}
          </span>
        </div>
        <div className="vbar-group">
          <span className="vbar-speed-wrap" ref={speedWrap}>
            <button
              className={"vbar-btn vbar-speed" + (rate !== 1 ? " changed" : "")}
              title={t.videoSpeed}
              aria-label={t.videoSpeed}
              aria-expanded={speedOpen}
              onClick={() => setSpeedOpen((o) => !o)}
            >
              {rate === 1 ? <Icon name="speed" size={24} /> : <span className="vbar-rate">{formatSpeed(rate, num)}</span>}
            </button>
            {speedOpen && (
              <div className="vbar-menu">
                {SPEEDS.map((s) => (
                  <button
                    key={s}
                    aria-pressed={s === rate}
                    className={"vbar-menu-item" + (s === rate ? " on" : "")}
                    onClick={() => {
                      video.playbackRate = s;
                      setSpeedOpen(false);
                    }}
                  >
                    {s === 1 ? t.videoSpeedNormal : formatSpeed(s, num)}
                  </button>
                ))}
              </div>
            )}
          </span>
          {pipAvailable && (
            <button
              className={"vbar-btn" + (pip ? " on" : "")}
              title={t.videoPip}
              aria-label={t.videoPip}
              aria-pressed={pip}
              onClick={() => {
                if (document.pictureInPictureElement === video) void document.exitPictureInPicture().catch(() => {});
                else void video.requestPictureInPicture().catch(() => {});
              }}
            >
              <Icon name="pip" size={24} />
            </button>
          )}
          <button
            className={"vbar-btn" + (fullscreen ? " on" : "")}
            title={t.viewerFullscreen}
            aria-label={t.viewerFullscreen}
            aria-pressed={fullscreen}
            onClick={onFullscreen}
          >
            <Icon name={fullscreen ? "fullscreenExit" : "fullscreen"} size={24} />
          </button>
        </div>
      </div>
    </div>
  );
}
