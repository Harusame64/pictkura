; NSISインストーラのフック。
;
; いまWiX断片（windows/autoplay-cleanup.wxs）でやっている
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
  ; 窓を出さずに解除だけして終わる入口（src-tauri/src/lib.rs の --unregister-autoplay）。
  ; 失敗してもアンインストールは続ける——解除できないことは、消せないことより軽い
  ExecWait '"$INSTDIR\pictkura.exe" --unregister-autoplay'
!macroend
