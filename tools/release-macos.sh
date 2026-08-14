#!/usr/bin/env bash
# macOS の配布物（pictkura.app を収めたZIP）を作る。
#
#   bash tools/release-macos.sh
#
# Windows 側の `release.ps1` と対になるもの。**UIのビルドを先に必ず走らせる**
# のが要点なのも同じ——`ui/dist` は生成物でリポジトリに入っていないため、
# 飛ばすと「ディスクにたまたま残っていた古いdist」がそのまま配布物へ入り、
# しかも何のエラーも出ない。
#
# DMGではなくZIPにしている。**Gatekeeperの観点では両者に差は無い**（未署名なら
# どちらも初回起動で弾かれる）ので、CIが単純な方を選んだ。DMGは後日の判断。
#
# 出力する形が Windows と違うのは、macOSでは `.app` が資源を内側に持つため。
# 取説とライセンス一覧は `pictkura.app/Contents/Resources/` に入り、
# アプリの「情報」画面はそこから開く（`lib.rs` の `about_info`）。
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

echo "== 1/3 UI をビルド =="
npm --prefix ui run build

echo "== 2/3 本体と .app をビルド =="
# `tauri.macos.conf.json` が `bundle.targets` を `app` に差し替える
# （既定の `msi` はmacOSでは作れない）。Tauriが自動で読む名前なので指定は要らない
(cd src-tauri && cargo tauri build)

echo "== 3/3 配布用のZIPを作る =="

app="target/release/bundle/macos/pictkura.app"
[ -d "$app" ] || { echo "アプリがない: $app （cargo tauri build に失敗している）" >&2; exit 1; }

# 版は tauri.conf.json を唯一の出どころにする（ここで二重管理しない）
version="$(node -p "require('./src-tauri/tauri.conf.json').version")"
arch="$(uname -m)"

stage="target/release/macos-stage/pictkura-$version"
rm -rf "$stage"
mkdir -p "$stage"

# **`ditto` を使う**。`.app` はシンボリックリンクと実行権限を含むので、
# `zip -r` では壊れる（起動しない `.app` が出来上がり、しかもZIPは正常に見える）
ditto "$app" "$stage/pictkura.app"
cp LICENSE THIRD-PARTY-LICENSES.txt "$stage/"

# 展開した人が最初に開くもの。**未署名なので初回は必ず弾かれる**——
# その回避手順がこのファイルの本題で、書き忘れると「壊れている」と思われて終わる。
# **`${version}` と波括弧で書く**こと。直後が全角文字だと `$version（` までを
# 変数名として読み、`set -u` で落ちる
cat > "$stage/はじめにお読みください.txt" <<EOF
pictkura ${version}（macOS版・Apple Silicon 専用）

インストールは要りません。pictkura.app を「アプリケーション」フォルダなど
好きな場所へ移してから実行してください。


■ 初回だけ、開き方に手順が要ります

このアプリはAppleの開発者署名を受けていないため、そのままダブルクリックすると
「"pictkura"は壊れているため開けません」と表示されます。壊れていません。

次のどちらかで開いてください。

  方法1: pictkura.app を右クリック（またはcontrolキーを押しながらクリック）して
         「開く」を選び、出てきた確認で「開く」を押す。

  方法2: 方法1で開けない場合は、ターミナルで次を実行してください。
         xattr -dr com.apple.quarantine /パス/pictkura.app

いずれも初回だけです。2回目からは普通にダブルクリックで開きます。


■ そのほか

- 使い方: アプリの「情報」画面から取扱説明書を開けます
- ライセンス: LICENSE（MIT）
- 同梱しているオープンソース: THIRD-PARTY-LICENSES.txt

設定と写真の索引は次の場所に作られます（写真そのものは入りません）。
  ~/Library/Application Support/dev.harusame.pictkura/

アンインストールは、pictkura.app と上の設定フォルダを消すだけです。


■ Windows版にあってmacOS版に無い機能

- 動画のサムネイル（一覧では既定の絵になります。再生はできます）
- HEIC/HEIFの表示
- クラウドにしか実体が無いファイルの撮影日などの取得
EOF

out="target/release/bundle/zip"
zip="$out/pictkura_${version}_${arch}.zip"
mkdir -p "$out"
rm -f "$zip"

# `--keepParent` で ZIP の中に `pictkura-$version/` を1階層残す。
# 展開したときにデスクトップへ中身が散らばらないようにするため
ditto -c -k --sequesterRsrc --keepParent "$stage" "$zip"

echo ""
echo "できたもの:"
ls -lh "$zip" | awk '{ printf "  %7s  %s\n", $5, $9 }'
