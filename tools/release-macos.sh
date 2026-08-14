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

# **配る対象はApple Siliconだけ**と決めてある（plan.md「macOSも配る」）。
# Intel機で走らせると「対応しないと明言した版」が黙って出来上がるので、先に止める。
#
# **ただしこれは機械の素性であって、作った物の素性ではない。** 本当の門は
# ビルド後の `lipo` の方（下）。Apple Silicon機でも rustup の既定ホストが
# `x86_64-apple-darwin` だと（Rosetta下で入れた場合に起きる）、`uname -m` は
# `arm64` を返すのに x86_64 のバイナリが出る。ここだけだと**それを
# `..._arm64.zip` という名前で配る**——止めたかったことがそのまま起きる（ゲート2の指摘）。
# ここに置いてあるのは、10分ぶんのビルドをする前に明らかなIntel機を弾くため
host_arch="$(uname -m)"
if [ "$host_arch" != "arm64" ]; then
    echo "Apple Silicon（arm64）でしか作れません: $host_arch" >&2
    echo "Intel版とUniversalは作らないと決めています（plan.md「macOSも配る」）。" >&2
    exit 1
fi

echo "== 1/3 UI をビルド =="
npm --prefix ui run build

echo "== 2/3 本体と .app をビルド =="
# `tauri.macos.conf.json` が `bundle.targets` を `app` に差し替える
# （既定の `msi` はmacOSでは作れない）。Tauriが自動で読む名前なので指定は要らない
(cd src-tauri && cargo tauri build)

echo "== 3/3 配布用のZIPを作る =="

app="target/release/bundle/macos/pictkura.app"
[ -d "$app" ] || { echo "アプリがない: $app （cargo tauri build に失敗している）" >&2; exit 1; }

# **作った物そのものの素性を見る。ここが本当の門。**
# ZIPの名前とREADMEの「Apple Silicon 専用」が指すのは機械ではなくバイナリなので、
# 名前もこの値から付ける——**素性と名前の出どころを1つにする**（食い違いようがなくなる）。
# `lipo -archs` はUniversalなら "arm64 x86_64" のように複数を返すので、
# 完全一致で見れば「arm64単体」以外は全部落ちる
arch="$(lipo -archs "$app/Contents/MacOS/pictkura")"
if [ "$arch" != "arm64" ]; then
    echo "arm64単体のバイナリではありません: $arch" >&2
    echo "rustup の既定ホストを確かめてください（rustup show）。" >&2
    echo "Apple Silicon機でも既定が x86_64-apple-darwin だと x86_64 が出ます。" >&2
    exit 1
fi

# 版は tauri.conf.json を唯一の出どころにする（ここで二重管理しない）
version="$(node -p "require('./src-tauri/tauri.conf.json').version")"

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

このアプリはAppleの開発者署名を受けていないため、そのままダブルクリックしても
起動せず、Gatekeeperに止められます。壊れてはいません。

まず pictkura.app を「アプリケーション」フォルダなど、置いておきたい場所へ
移してください。これは作法ではありません。移さずに起動すると、macOSはアプリを
読み取り専用の一時領域へ写して実行します（App Translocation）。

そのうえで、お使いの macOS の版によって手順が違います。


● macOS 15（Sequoia）以降

ダブルクリックすると、こう表示されます。

  "pictkura" は開いていません
  Apple は、"pictkura" に Mac に損害を与えたり、プライバシーを侵害する
  可能性のあるマルウェアが含まれていないことを検証できませんでした。
      ［ゴミ箱に入れる］　［完了］

このダイアログに先へ進むボタンはありません。次の手順で開いてください。

  1. ［完了］を押す
  2. システム設定 →「プライバシーとセキュリティ」を開き、下の方までスクロールする
  3. pictkura がブロックされた旨の行にある「このまま開く」を押す
  4. パスワードを求められたら入れる。アプリが起動します

手順2の行は、一度ダブルクリックして弾かれたあとにしか出ません。順番どおりに。


● macOS 11〜14

pictkura.app を右クリック（controlキーを押しながらクリック）して「開く」を選び、
出てきた確認で「開く」を押してください。

※ この抜け道は macOS 15 で廃止されました。逆に、上の「システム設定」の手順は
   画面の名前が違う macOS 11〜12 には当てはまりません。版に合った方をご覧ください。


● どの版でも使える方法（ターミナル）

  xattr -dr com.apple.quarantine /パス/pictkura.app


初回だけです。2回目からは普通にダブルクリックで開きます。


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

# **今の版のファイルだけを消すのでは足りない。置き場ごと作り直す。**
# `target/` はCIのキャッシュから復元されるので、版を上げた最初の実行では
# 前の版のZIPが隣に残る。すると集める側の `ls .../*.zip` が2行返り、
# 配布物の員数確認（4つ）も5つに見えて落ちる（ゲート2の指摘）
out="target/release/bundle/zip"
rm -rf "$out"
mkdir -p "$out"
zip="$out/pictkura_${version}_${arch}.zip"

# `--keepParent` で ZIP の中に `pictkura-$version/` を1階層残す。
# 展開したときにデスクトップへ中身が散らばらないようにするため
ditto -c -k --sequesterRsrc --keepParent "$stage" "$zip"

echo ""
echo "できたもの:"
ls -lh "$zip" | awk '{ printf "  %7s  %s\n", $5, $9 }'
