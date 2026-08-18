; NSISインストーラのフック。
;
; WiX断片（windows/autoplay-cleanup.wxs）でやっている
; 「アンインストール前に AutoPlay の登録を消す」を、NSIS側の同じ位置へ移したもの。
;
; なぜ要るか: AutoPlayの登録は**アプリが起動のたびに実行時にHKCUへ書く**もので、
; インストーラが作ったものではない。消さずにアンインストールすると、
; USB/SDカードを挿すたびに「pictkura で写真を取り込む」が候補に並び、
; 選ぶと**もう居ない実行ファイル**を呼ぶ。
;
; NSIS_HOOK_PREUNINSTALL は「ファイル・レジストリ・ショートカットを消す前」に走るので、
; まだ在る pictkura.exe を呼べる（WiXで `Before="RemoveFiles"` に置いたのと同じ位置）。

!macro NSIS_HOOK_PREUNINSTALL
  ; **更新のときは解除しない**。新しい版を入れる前に、インストーラが古い版の
  ; アンインストーラを呼ぶ。素通しすると**更新のたびに登録が消え**、次に
  ; アプリを起動するまで自動再生の候補から居なくなる（完了画面の「起動する」を
  ; 外した人はそのまま気づかない）。WiX側の `NOT UPGRADINGPRODUCTCODE` と同じ意図。
  ;
  ; 見分け方は**どこから走っているか**。NSISのアンインストーラは、普通に起動されると
  ; **自分をtempへ複製して走り直す**（そうしないと導入先ごと消せない）。
  ; インストーラが更新のために呼ぶときは `_?=<導入先>` を付けるので、複製せず
  ; **その場で**走る。つまり:
  ;
  ;   $EXEDIR == $INSTDIR  → インストーラから呼ばれた（更新）
  ;   $EXEDIR != $INSTDIR  → 利用者が「設定 → アプリ」等から消した
  ;
  ; **`$CMDLINE` を見てはいけない**——NSISは `_?=` をそこから取り除く。実測では
  ; 更新の呼び出しでも `["…\pictkura\uninstall.exe" /S]` としか入っておらず、
  ; 利用者による削除では `["…\Temp\~nsu2.tmp\Un.exe"  /S]` になる（差はパスのほう）
  ${If} $EXEDIR == $INSTDIR
    DetailPrint "更新のため、AutoPlayの登録はそのまま残します"
  ${Else}
    ; 窓を出さずに解除だけして終わる入口（src-tauri/src/lib.rs の --unregister-autoplay）。
    ; 失敗してもアンインストールは続ける——解除できないことは、消せないことより軽い
    ExecWait '"$INSTDIR\pictkura.exe" --unregister-autoplay'
  ${EndIf}
!macroend
