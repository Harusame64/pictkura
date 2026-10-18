; NSISインストーラのフック。AutoPlay（自動再生）の登録と、winget のための QuietUninstallString を書く。
;
; **このファイルは UTF-8（BOM つき）で保つ。** 下の登録に日本語の文字列があり、NSIS は BOM の無いファイルを
; システムの文字コード（CI の英語版 Windows なら cp1252）で読む。
;
; **AutoPlay の登録はインストーラーが書く**（2026-10-18 利用者の決定。pictkura-dev #53、dev/plan.msix.md）。
; アプリは実行中にレジストリへ登録を書かない。MSI 版は windows/autoplay.wxs、Store 版はマニフェスト、持ち歩き版は
; 候補に出さない。名前は前の版がアプリから書いた登録（pictkuraImportFromDrive）と分ける——アプリが起動時に前の版の
; ぶんを片付けるとき、こちらを巻き添えにしない。currentUser のインストールなので SHCTX は HKCU。
;
; やってみて分かったこと（同じ道を二度通らないための記録）:
;
; - **`$CMDLINE` から `_?=` は探せない**。NSISがそこから取り除く。実測では更新の
;   呼び出しでも `["…\pictkura\uninstall.exe" /S]` としか入っておらず、利用者による
;   削除では `["…\Temp\~nsu2.tmp\Un.exe"  /S]` になる（差はパスのほう）
; - **アンインストールの後（POSTUNINSTALL）で exe を呼ぶのは駄目だった**。そのときには `pictkura.exe` が
;   消えていて、`$PLUGINSDIR` へ写した exe は隣の `WebView2Loader.dll` が無くて起動できず、
;   **アンインストーラごと固まった**。exe を呼ぶ片付けは PREUNINSTALL（まだ在る）で行う

!define PK_AUTOPLAY "Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers"
!define PK_AUTOPLAY_HANDLER "pictkuraAutoPlay"
!define PK_AUTOPLAY_PROGID "pictkura.AutoPlay"
!define PK_AUTOPLAY_ACTION "pictkura で写真を取り込む"

!macro NSIS_HOOK_PREUNINSTALL
  ; **インストーラから呼ばれたときは何もしない。** アプリは消えるのではなく
  ; 入れ替わるので、ここで解除すると「常にこの操作」で pictkura を選んでいた記録まで
  ; 消える（`--unregister-autoplay` は、もう居ないハンドラを呼び続けないよう
  ; その記録も消す作りになっている）。**更新の既定は「先にアンインストール」**なので、
  ; 素通しすると**更新のたびに選び直させる**——MSIでは `NOT UPGRADINGPRODUCTCODE`
  ; で守られていた性質（PR #24 のゲート1 P2）。
  ;
  ; 見分け方は**どこから走っているか**。NSISのアンインストーラは、普通に起動されると
  ; **自分をtempへ複製して走り直す**（そうしないと導入先ごと消せない）。
  ; インストーラから呼ばれるときは `_?=<導入先>` が付くので複製せず**その場で**走る:
  ;
  ;   $EXEDIR == $INSTDIR  → インストーラが更新のために呼んだ
  ;   $EXEDIR != $INSTDIR  → 利用者が「設定 → アプリ」等から消した
  ;
  ; （**MSIからの乗り換えはここを通らない**。消すのは msiexec で、こちらの
  ;   アンインストーラは走らない。MSI の登録（HKLM）は MSI が消し、こちらの POSTINSTALL が HKCU に書く）
  ;
  ; **残る穴（承知のうえ・2つ）**:
  ;
  ; 1. `-setup.exe` を実行して「アンインストールする」を選び、**その後インストールを
  ;    続けずに閉じた**ときは、アプリが消えたのに登録が残る（ゲート2のP3）。普通の
  ;    削除口は「設定 → アプリ」で、そちらは正しく消える
  ; 2. 解除は、テンプレートの「pictkura を閉じますか」より**前**に走る（フックの位置は
  ;    Tauri側で固定）。アプリを動かしたまま消し始めて**そこでキャンセル**すると、
  ;    アプリも候補も残るが、**「常にこの操作」の記録だけ消える**（選び直しが要る）。候補そのものは
  ;    POSTUNINSTALL で消すので残る
  ;
  ; どちらも塞ぐには、「常にこの操作」の記録を控えて戻す仕掛けが要る——別のPRで。
  ${If} $EXEDIR != $INSTDIR
    ; 前の版の登録と、新しい候補を指す「常にこの操作」を消す（exe がまだ在るうちに。src-tauri/src/lib.rs の
    ; --unregister-autoplay）。残すとエクスプローラーはもう居ないハンドラを呼び続ける。
    ; 失敗してもアンインストールは続ける——解除できないことは、消せないことより軽い
    ExecWait '"$INSTDIR\pictkura.exe" --unregister-autoplay'
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; **登録そのものは消し終わってから消す**（このインストーラーが書いたもの）。PREUNINSTALL で消すと、テンプレートの
  ; 「pictkura を閉じますか」でキャンセルしたとき、アプリは残ったまま候補だけ消え、入れ直すまで戻らない
  ; （アプリは登録を書かない。ゲート2）。ここでは exe を呼ばない——上の記録のとおり固まる。
  ; 更新（$EXEDIR == $INSTDIR）では消さない——すぐ POSTINSTALL が書き直すが、間の時間を作らない
  ${If} $EXEDIR != $INSTDIR
    DeleteRegValue SHCTX "${PK_AUTOPLAY}\EventHandlers\ShowPicturesOnArrival" "${PK_AUTOPLAY_HANDLER}"
    DeleteRegValue SHCTX "${PK_AUTOPLAY}\EventHandlers\MixedContentOnArrival" "${PK_AUTOPLAY_HANDLER}"
    DeleteRegValue SHCTX "${PK_AUTOPLAY}\EventHandlers\StorageOnArrival" "${PK_AUTOPLAY_HANDLER}"
    DeleteRegKey SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}"
    DeleteRegKey SHCTX "Software\Classes\${PK_AUTOPLAY_PROGID}"

    ; **インストール先の記録も消す**（pictkura-dev #70）。Tauri のアンインストーラーは「アプリのデータも消す」に
    ; チェックしたときしか消さず、残った既定値を **MSI が前のインストール先として拾う**——全員向けの MSI が
    ; この利用者の AppData\Local に入り、ほかの利用者は使えない。データ（%APPDATA%）には触らない
    DeleteRegValue SHCTX "${MANUPRODUCTKEY}" ""
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; **`QuietUninstallString` を自分で書く。** テンプレートが書くのは
  ; `UninstallString`（`"$INSTDIR\uninstall.exe"`）**だけ**で、画面を出さない方は
  ; 空のまま（tauri-bundler 2.9.4 の `installer.nsi`。同じ行で `DisplayName` や
  ; `InstallLocation` は書いている）。
  ;
  ; winget は `QuietUninstallString` があればそれを使い、**無ければ
  ; `UninstallString` をそのまま叩く**。つまり空のままだと
  ; `winget uninstall pictkura` が**アンインストーラの窓を出す**——
  ; 無人で消せることは winget の前提なので、ここを埋めるまで載せられない。
  ;
  ; **直す先がマニフェストではなくアプリ側**なのは、winget のマニフェストに
  ; この値を書く欄が無く、**入った先のレジストリを読む**から。
  ;
  ; `/S` で本当に消えること自体は実機で確認済み——終了コード0・1.7秒・残骸なし
  ; （`dev/winget/README.md` の3）。ここでやるのは、その呼び方を登録に置くだけ。
  ;
  ; **置き場所**: このフックはテンプレートの登録が済んだ後に走る（`installer.nsi` の
  ; Section 末尾で、`UninstallString` を書く行より後）。同じキーへ足すだけなので
  ; 上書きにはならない。**下の `ExecWait` より前に置く**——外部プロセスの呼び出しが
  ; 転んでも、登録だけは正しくしておきたいため
  !ifndef UNINSTKEY
    ; **黙って別のキーへ書かせない。** NSIS は未定義の `${...}` を文字列として
    ; そのまま通すので、テンプレート側で名前が変わると
    ; `${UNINSTKEY}` という名前のキーが出来て、**誰も気づかない**。
    ; 版を上げたときにここで止まるほうがよい
    !error "UNINSTKEY が未定義。tauri-bundler の installer.nsi で定義名を確かめること"
  !endif
  WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"

  ; **AutoPlay の候補を登録する**（このインストーラーが書き、アンインストールで消す）。起動の引数は MSI 版・Store 版と
  ; 同じ。対象のパスは引用符で囲む（フォルダにマウントされたボリュームでは空白を含みうる。受け側の
  ; import_path_from_args が末尾のバックスラッシュの崩れを戻す）。版上げでも書き直すので、導入先が変わっても追従する
  WriteRegStr SHCTX "Software\Classes\${PK_AUTOPLAY_PROGID}" "" "${PK_AUTOPLAY_ACTION}"
  WriteRegStr SHCTX "Software\Classes\${PK_AUTOPLAY_PROGID}\shell\import\command" "" '"$INSTDIR\pictkura.exe" --import "%L"'
  WriteRegStr SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}" "Action" "${PK_AUTOPLAY_ACTION}"
  WriteRegStr SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}" "Provider" "pictkura"
  WriteRegStr SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}" "InvokeProgID" "${PK_AUTOPLAY_PROGID}"
  WriteRegStr SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}" "InvokeVerb" "import"
  WriteRegStr SHCTX "${PK_AUTOPLAY}\Handlers\${PK_AUTOPLAY_HANDLER}" "DefaultIcon" "$INSTDIR\pictkura.exe,0"
  WriteRegStr SHCTX "${PK_AUTOPLAY}\EventHandlers\ShowPicturesOnArrival" "${PK_AUTOPLAY_HANDLER}" ""
  WriteRegStr SHCTX "${PK_AUTOPLAY}\EventHandlers\MixedContentOnArrival" "${PK_AUTOPLAY_HANDLER}" ""
  WriteRegStr SHCTX "${PK_AUTOPLAY}\EventHandlers\StorageOnArrival" "${PK_AUTOPLAY_HANDLER}" ""

  ; **前の版がアプリから書いた登録を消す**（旧い候補と新しい候補が並ぶ時間を作らない）。新しい登録の
  ; 「常にこの操作」には触らない——版上げのたびに選び直させない
  ExecWait '"$INSTDIR\pictkura.exe" --remove-legacy-autoplay'
!macroend
