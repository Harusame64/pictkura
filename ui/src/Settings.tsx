import { useCallback, useEffect, useRef, useState } from "react";
// `open` は下のプロパティ名とぶつかるので別名で受ける
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  aboutInfo,
  checkUpdate,
  forgetEditor,
  openDownloadPage,
  setCheckUpdateOnStart,
  listFolderPatterns,
  openBundledDoc,
  previewFolderPattern,
  setFolderPattern,
  setImportDestination,
  setAutoAdvance,
  setRegisterAutoplay,
  type AboutInfo,
  type AppConfig,
  type UpdateCheck,
  type ExternalApp,
  type FolderPattern,
} from "./api";
import { usePlatform } from "./usePlatform";
import {
  LOCALES,
  locale,
  readLocaleChoice,
  setLocaleChoice,
  t,
} from "./i18n";
import { applyTheme, readTheme, type ThemeChoice } from "./theme";

/**
 * 設定ダイアログ。
 *
 * 「最小機能」の方針どおり、設定は**迷いどころだけ**を出す:
 * 取り込み先のフォルダ構成（一度決めたら数千枚に効くので後から直しにくい）、
 * 見た目のテーマ、登録済みの編集アプリ。それ以外はTOMLを直接編集できる。
 */
export default function Settings({
  open,
  onClose,
  config,
  onConfigChanged,
  onError,
}: {
  open: boolean;
  onClose: () => void;
  config: AppConfig | null;
  onConfigChanged: () => void;
  onError: (message: string) => void;
}) {
  // AutoPlayを出すかの正。問い合わせは `usePlatform` に1つだけ持たせてある
  const platform = usePlatform();
  const [patterns, setPatterns] = useState<FolderPattern[]>([]);
  /** コピー先の変更が断られた理由（ダイアログ内に出す） */
  const [destError, setDestError] = useState<string | null>(null);
  const [theme, setTheme] = useState<ThemeChoice>(readTheme);
  /** 自由記述の入力欄を開いているか、その中身と、実際にできるフォルダ名 */
  const [customMode, setCustomMode] = useState(false);
  const [custom, setCustom] = useState("");
  const [customPreview, setCustomPreview] = useState("");
  const [about, setAbout] = useState<AboutInfo | null>(null);
  /**
   * 選ばれている言語。**stateに持つのが要点**。`select` は制御された部品なので、
   * 選んでも再レンダーが起きないと React が DOM を前の値へ戻す。言語が実際には
   * 変わらない選び方（日本語OSで「OSに合わせる」→「日本語」など）では
   * 読み込み直しも起きないため、保存はされているのに表示だけ戻り、
   * 「効かなかった」ように見えていた。
   */
  const [langChoice, setLangChoice] = useState<string | null>(readLocaleChoice);
  /**
   * 「更新を確認」を押したときの返事（0.2）。`null` はまだ押していない、
   * `"checking"` は問い合わせ中、文字列はそのまま出す一行。
   * **押したときは必ず何か返す**——押しても何も変わらないのが一番分かりにくい。
   */
  const [updateSay, setUpdateSay] = useState<string | null>(null);
  const [updateNewer, setUpdateNewer] = useState<UpdateCheck | null>(null);
  /** 問い合わせ中か。**文言の一致では見ない**（言語を足すと壊れる・ゲート2） */
  const [updateChecking, setUpdateChecking] = useState(false);

  useEffect(() => {
    if (!open) return;
    listFolderPatterns()
      .then(setPatterns)
      .catch(() => {});
    aboutInfo()
      .then(setAbout)
      .catch(() => {});
  }, [open]);

  // 設定にあるパターンがプリセットのどれでもなければ、自由記述として開く。
  //
  // **開いた1回だけ**初期化する（入力中に設定が変わって打鍵を奪わないように）。
  // `config` が届く前に走らせないのも要点で、null のまま初期化すると
  // 「振り分けない」が現在の構成として表示され、次の操作で本物を潰す。
  const initialised = useRef(false);
  useEffect(() => {
    if (!open) {
      initialised.current = false;
      // 閉じても状態は残る（`!open` で null を返すだけ）ので、
      // 前回の拒否メッセージを次に開いたとき出さないよう消す
      setDestError(null);
      return;
    }
    if (initialised.current || patterns.length === 0 || !config) return;
    initialised.current = true;
    const saved = config.routing.folder_pattern;
    const isPreset = patterns.some((p) => p.pattern === saved);
    setCustomMode(!isPreset);
    // **空で始めない**。プリセットを使っている人が「自分で決める」を覗いただけで
    // 空が保存されると、以後の取り込みがコピー先直下へ平積みになる
    setCustom(saved);
  }, [open, patterns, config]);

  // できるフォルダ名は**Rust側に作らせる**。置換だけでなく無害化（`..`や
  // 使えない文字を落とす）まで通った結果を見せないと、打った通りにならない
  useEffect(() => {
    if (!customMode) {
      setCustomPreview("");
      return;
    }
    let cancelled = false;
    previewFolderPattern(custom)
      .then((rendered) => {
        if (!cancelled) setCustomPreview(rendered);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [custom, customMode]);

  /** 最後に書いた値。入力欄を離れるのと閉じるので二重に書かないための印 */
  const lastWritten = useRef<string | null>(null);

  // 自由記述の保存はここだけを通す。
  //
  // - **空は書かない**。「振り分けない」にしたい人はプリセットの側を選ぶ。
  //   空を保存できてしまうと、覗いただけで日付フォルダが消える
  // - 現在の設定と同じなら書かない（ファイル書き込みと再取得が無駄に走る）
  const commitCustom = useCallback(() => {
    const saved = config?.routing.folder_pattern ?? "";
    const value = custom.trim();
    if (!customMode || value === "" || value === saved || value === lastWritten.current) {
      return;
    }
    lastWritten.current = value;
    void setFolderPattern(value)
      .then(onConfigChanged)
      .catch(() => {
        // 書けなかったら印も戻す（次の操作でやり直せるように）
        lastWritten.current = null;
      });
  }, [customMode, custom, config, onConfigChanged]);

  // 閉じる経路はここに集約する。**入力中の自由記述を捨てない**のが目的で、
  // ダイアログが消えると入力欄の onBlur は飛ばないため、閉じる側でも確定させる
  const closeDialog = useCallback(() => {
    commitCustom();
    onClose();
  }, [commitCustom, onClose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeDialog();
    };
    if (open) window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, closeDialog]);

  if (!open) return null;

  /**
   * 開くべき取扱説明書の種類（同梱されていなければ `null`）。
   *
   * 説明書がある言語で使っているならその言語のものを、無ければ**英語へ落とす**。
   * 日本語版へ落とさないのは、**言語を増やしたときに読めない確率が低い方**を
   * 選ぶため（`pickLocale` の既定が英語なのと同じ考え）。
   */
  const manualDoc: "manual" | "manual-en" | null = (
    locale.startsWith("ja")
      ? [
          ["manual", about?.manual_path],
          ["manual-en", about?.manual_en_path],
        ]
      : [
          ["manual-en", about?.manual_en_path],
          ["manual", about?.manual_path],
        ]
  ).find(([, path]) => path)?.[0] as "manual" | "manual-en" | undefined ?? null;

  const current = config?.routing.folder_pattern ?? "";
  const destination = config?.routing.destination ?? null;
  const editors: ExternalApp[] = config?.editors?.apps ?? [];

  const choose = async (pattern: string) => {
    try {
      await setFolderPattern(pattern);
      onConfigChanged();
    } catch {
      /* 保存失敗は設定を変えない（次の操作で再試行できる） */
    }
  };

  return (
    <div className="palette-backdrop" onClick={closeDialog}>
      <div className="settings" onClick={(e) => e.stopPropagation()}>
        <div className="settings-head">
          <h2>{t.settingsTitle}</h2>
          <button className="settings-close" onClick={closeDialog} title={t.close}>
            ✕
          </button>
        </div>
        <div className="settings-body">
          <section className="settings-section">
            <h3>{t.settingsImportStructure}</h3>
            <p className="settings-note">{t.settingsImportStructureNote}</p>
            <div className="settings-dest">
              {t.settingsDestination}:{" "}
              <code>{destination ?? t.settingsDestinationUnset}</code>
              <button
                className="settings-dest-change"
                onClick={async () => {
                  const dest = await openDialog({
                    directory: true,
                    title: t.pickDestination,
                  });
                  if (typeof dest !== "string") return;
                  // 選んだ先が消えている・ネットワークが切れている・
                  // 写真.appのライブラリの中だった等で失敗しうる。
                  // 設定は変えないまま、**理由は出す**（黙って何も起きないと
                  // 押し損ねたのか断られたのか分からない）
                  try {
                    await setImportDestination(dest);
                  } catch (e) {
                    // **ダイアログの中に出す。** 画面下の状態バーへ流しても
                    // このダイアログが覆っているうえ32chで省略されるので、
                    // 断られた理由もパスも読めない
                    setDestError(String(e));
                    onError(String(e));
                    return;
                  }
                  setDestError(null);
                  onConfigChanged();
                }}
              >
                {t.wizardChangeDestination}
              </button>
            </div>
            {destError && <p className="settings-error">{destError}</p>}
            <div className="pattern-list">
              {patterns.map((p) => (
                <label
                  key={p.pattern || "(flat)"}
                  className={"pattern-item" + (!customMode && p.pattern === current ? " active" : "")}
                >
                  <input
                    type="radio"
                    name="folder-pattern"
                    checked={!customMode && p.pattern === current}
                    onChange={() => {
                      setCustomMode(false);
                      choose(p.pattern);
                    }}
                  />
                  <code className="pattern-example">
                    {p.example
                      ? `${p.example}/IMG_0001.JPG`
                      : t.settingsFlatExample}
                  </code>
                </label>
              ))}
              <label
                className={"pattern-item" + (customMode ? " active" : "")}
              >
                <input
                  type="radio"
                  name="folder-pattern"
                  checked={customMode}
                  onChange={() => setCustomMode(true)}
                />
                <span className="pattern-custom-label">
                  {t.settingsCustomPattern}
                </span>
              </label>
              {customMode && (
                <div className="pattern-custom">
                  <input
                    type="text"
                    className="pattern-custom-input"
                    value={custom}
                    placeholder="{year}/{month}"
                    spellCheck={false}
                    autoFocus
                    onChange={(e) => setCustom(e.target.value)}
                    // 打鍵のたびに設定ファイルを書かない。離れたとき・Enterで保存する
                    onBlur={commitCustom}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        (e.target as HTMLInputElement).blur();
                      }
                    }}
                  />
                  <p className="settings-note">{t.settingsCustomPatternNote}</p>
                  <div className="pattern-custom-preview">
                    {t.settingsCustomPatternResult}:{" "}
                    <code>
                      {customPreview
                        ? `${customPreview}/IMG_0001.JPG`
                        : t.settingsFlatExample}
                    </code>
                  </div>
                </div>
              )}
            </div>
          </section>

          {/*
            全画面ビューアの選別（0.2 ②）。キーバインドは既定の1種だけを持ち、
            プリセット機構は作らない（`dev/plan.0.2.rev.md` 決定事項3）ので、
            設定に出すのは自動送りの入切だけ。
          */}
          <section className="settings-section">
            <h3>{t.settingsViewer}</h3>
            <label className="settings-toggle">
              <input
                type="checkbox"
                // 配ったあとに足した節なので、古い設定ファイルには無い。
                // Rust側の既定（ON）に合わせる
                checked={config?.viewer?.auto_advance ?? true}
                onChange={async (e) => {
                  try {
                    await setAutoAdvance(e.target.checked);
                  } catch (err) {
                    // 保存できなければ下の onConfigChanged で表示が元に戻る
                    onError(String(err));
                  }
                  onConfigChanged();
                }}
              />
              {t.settingsAutoAdvanceToggle}
            </label>
            <p className="settings-note">{t.settingsAutoAdvanceNote}</p>
          </section>

          {/*
            AutoPlayはWindowsだけの機構（macOS/Linuxでは登録処理そのものが
            何もしない）。効かない設定は出さない。

            **判定はバックエンドの `cfg!`**（`getHostPlatform`）。`isWindows` は
            `navigator.userAgent` を見るので、WebViewのUA次第で外れうる
            ——外れると**Windowsの利用者からトグルごと消える**。文言が違うより悪い
            （ゲート2の指摘）。届くまでは推測で出しておく
          */}
          {platform === "windows" && (
            <section className="settings-section">
              <h3>{t.settingsAutoplay}</h3>
              <label className="settings-toggle">
                <input
                  type="checkbox"
                  checked={config?.import.register_autoplay ?? true}
                  onChange={async (e) => {
                    try {
                      await setRegisterAutoplay(e.target.checked);
                    } catch (err) {
                      // レジストリに書けなかった場合。設定は保存されていないので
                      // 下の onConfigChanged で表示は元の状態に戻る
                      onError(String(err));
                    }
                    onConfigChanged();
                  }}
                />
                {t.settingsAutoplayToggle}
              </label>
              <p className="settings-note">{t.settingsAutoplayNote}</p>
            </section>
          )}

          <section className="settings-section">
            {/*
              見出しはそこに文字が並んでいるだけで、`select` の名前にはならない
              （名前の無い「コンボボックス」と読まれる）。`aria-label` で同じ文字を
              別に持たせる手もあるが、**見えているラベルをそのまま指す**方が
              二重管理にならず、表示と読み上げが食い違わない。

              **閉じたままの矢印キーは1段ごとに確定する**（Chromium系の作法）。
              2段先を狙うと途中で読み込み直しが挟まるが、Alt+↓ で開いてから選べば
              確定は1回で済む。**言語が増えると、この寄り道も長くなる**——
              いまは7つ（2026-09-01。中国語2つを足した時点）で、端から端まで
              矢印で送ると読み込み直しが6回挟まる。それでも「適用」釦は置かない:
              開いて選べば1回で済むし、釦を置くと**選んだのに反映されていない状態**を
              新しく作ることになる。ここが辛くなったら、確定を遅らせる側で直す。

              **`lang` を各選択肢に付ける**。中国語のラベル（`简体中文` / `繁體中文`）は
              日本語の画面にも出るが、`简` は日本語の書体に無いので**1語が2書体**で
              組まれる（`tokens.css` の `:lang()` が直したのと同じ問題）。
            */}
            <h3 id="settings-language-label">{t.settingsLanguage}</h3>
            <select
              className="settings-select"
              aria-labelledby="settings-language-label"
              value={langChoice ?? ""}
              onChange={(e) => {
                const code = e.target.value === "" ? null : e.target.value;
                // **保存できたときだけ表示を進める**。保存に失敗すると言語は
                // 変わらないので、先に表示だけ変えると「その言語になっている」と
                // 嘘をつくことになる。state を変えなければ、Reactが
                // プルダウンを元の選択へ戻してくれる
                if (setLocaleChoice(code)) setLangChoice(code);
              }}
            >
              <option value="">{t.settingsLanguageSystem}</option>
              {LOCALES.map((l) => (
                <option key={l.code} value={l.code} lang={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
            <p className="settings-note">{t.settingsLanguageNote}</p>
          </section>

          <section className="settings-section">
            <h3>{t.settingsTheme}</h3>
            <div className="theme-switch">
              {(["system", "light", "dark"] as ThemeChoice[]).map((choice) => (
                <button
                  key={choice}
                  className={theme === choice ? "active" : ""}
                  onClick={() => {
                    setTheme(choice);
                    applyTheme(choice);
                  }}
                >
                  {choice === "system"
                    ? t.themeSystem
                    : choice === "light"
                      ? t.themeLight
                      : t.themeDark}
                </button>
              ))}
            </div>
          </section>

          {editors.length > 0 && (
            <section className="settings-section">
              <h3>{t.settingsEditors}</h3>
              <p className="settings-note">{t.settingsEditorsNote}</p>
              {editors.map((app) => (
                <div key={app.path} className="editor-row">
                  <span className="editor-name">{app.name}</span>
                  <code className="editor-path">{app.path}</code>
                  <button
                    onClick={() =>
                      forgetEditor(app.path).then(onConfigChanged).catch(() => {})
                    }
                    title={t.settingsForgetEditor}
                  >
                    ✕
                  </button>
                </div>
              ))}
            </section>
          )}
          <section className="settings-section">
            <h3>{t.settingsAbout}</h3>
            <div className="about-row">
              <span className="about-name">pictkura</span>
              <code className="about-version">v{about?.version ?? "?"}</code>
              {/* 新しい版の確認（0.2）。押した回は間隔も入切も無視して聞きに行く */}
              <button
                className="about-check"
                disabled={updateChecking}
                onClick={async () => {
                  setUpdateChecking(true);
                  setUpdateSay(t.updateChecking);
                  setUpdateNewer(null);
                  try {
                    const r = await checkUpdate(true);
                    if (r.newer) {
                      setUpdateNewer(r);
                      setUpdateSay(t.updateFound(r.latest ?? ""));
                    } else {
                      setUpdateSay(t.updateUpToDate);
                    }
                  } catch {
                    // 繋がらない・弾かれたの区別は利用者の用事ではない。
                    // 「確認できませんでした」の一行で足りる
                    setUpdateSay(t.updateFailed);
                  } finally {
                    setUpdateChecking(false);
                  }
                }}
              >
                {t.updateCheckNow}
              </button>
            </div>
            {updateSay && (
              <p className="settings-note update-say">
                {updateSay}
                {updateNewer && (
                  <button onClick={() => openDownloadPage().catch(() => {})}>
                    {t.updateOpenPage}
                  </button>
                )}
              </p>
            )}
            <label className="settings-toggle">
              <input
                type="checkbox"
                // 配ったあとに足した節なので、古い設定ファイルには無い。
                // Rust側の既定（ON）に合わせる
                checked={config?.update?.check_on_start ?? true}
                onChange={async (e) => {
                  try {
                    await setCheckUpdateOnStart(e.target.checked);
                  } catch (err) {
                    onError(String(err));
                  }
                  onConfigChanged();
                }}
              />
              {t.updateOnStart}
            </label>
            <p className="settings-note">{t.updateOnStartNote}</p>
            <p className="settings-note">{t.settingsAboutLicense}</p>
            <div className="about-links">
              {/*
                取扱説明書は表示言語で出し分ける。英語版が同梱されていない実行では
                日本語版へ落とす——**ボタンを消さない**のが要点で、説明書が open
                できないより、読める言語でないものが開く方がまだ手掛かりになる。
              */}
              <button
                disabled={!manualDoc}
                title={
                  (manualDoc === "manual-en"
                    ? about?.manual_en_path
                    : about?.manual_path) ?? t.settingsDocNotBundled
                }
                onClick={() =>
                  manualDoc && openBundledDoc(manualDoc).catch(() => {})
                }
              >
                {t.settingsManual}
              </button>
              <button
                disabled={!about?.licenses_path}
                title={about?.licenses_path ?? t.settingsDocNotBundled}
                onClick={() => openBundledDoc("licenses").catch(() => {})}
              >
                {t.settingsOssLicenses}
              </button>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
