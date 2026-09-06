# インストール不要で動く「持ち歩き版」のZIPを作る。
#
#   pwsh tools/release.ps1
#
# **入口はこれだけで、単体で呼ぶ道は残していない。** ここは**その場所の実行ファイルを
# そのまま入れる**だけなので、release.ps1 から呼ばれたときは**インストーラの中身と
# 同じバイト列**が入る（あちらが目印を潰してある）。**素の `cargo tauri build` の
# あとに走らせるとそうはならない**——バンドラが束ね終わりに原本へ戻すので、
# **ZIP だけ3バイト違う**兄弟になる。だから下で目印を見て断る。
#
# MSIと同じものを、レジストリにもスタートメニューにも触らない形で配る。
# 中身の並びは**インストール後と同じ**にする——アプリは実行ファイルの隣から
# 取扱説明書とライセンス一覧を探すので、ここを崩すと「同梱されていません」になる。
$ErrorActionPreference = "Stop"

# 目印の語と、バイト列を探す道具は **release.ps1 と共有する**。
. (Join-Path $PSScriptRoot "bundle-token.ps1")

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    # **置き場は決め打ちしない**（`CARGO_TARGET_DIR` で動く）。release.ps1 と同じ理由で、
    # ここだけ別の場所を見ると**インストーラと違うバイト列が ZIP に入る**。
    $targetDir = (cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json).target_directory
    if ($LASTEXITCODE -ne 0 -or -not $targetDir) { throw "cargo metadata から置き場を取れない" }
} finally { Pop-Location }
if ($env:CARGO_BUILD_TARGET) {
    throw "CARGO_BUILD_TARGET が設定されている（$env:CARGO_BUILD_TARGET）——cargo と cargo tauri bundle が別の置き場を見るので、この工程は支えていません"
}
$releaseDir = Join-Path $targetDir "release"
$exe = Join-Path $releaseDir "pictkura.exe"

# **release.ps1 と同じ理由で、置き場の形も見る**（三つ組が効いていると
# `target\<triple>\release` になり、ここは古い実行ファイルを掴む）。
$builds = @(Get-ReleaseBinaryCandidates $targetDir "pictkura.exe")
if ($builds.Count -eq 0) {
    throw "実行ファイルがない: $exe （先に pwsh tools/release.ps1 を実行してください）"
}
if ($builds.Count -gt 1) {
    throw "実行ファイルが $($builds.Count) 箇所にある——どれを入れるのか決められない: $($builds.FullName -join ', ')"
}
if ($builds[0].FullName -ne $exe) {
    throw "実行ファイルの置き場が想定と違う: $($builds[0].FullName)。build.target で三つ組を指定していると target\<triple>\release になります——この工程はその形を支えていません"
}

# **インストーラと同じバイト列か、ここで確かめる。**
# 素の `cargo tauri build` のあとに単体で走らせると、バンドラが束ね終わりに
# 原本へ戻すので、**この実行ファイルだけ3バイト違う**（`..._UNK` のまま）。
# 黙って違う ZIP を作るより、**作らずに理由を言うほうがよい**——手元で作った物は
# CI の門を通らないので、**気づく機会がここしか無い。**
$blob = [IO.File]::ReadAllBytes($exe)
$found = Test-BundleToken $blob $BundleTokenReplacement
$blob = $null
if (-not $found) {
    throw "この実行ファイルは release.ps1 を通っていない（目印が入っていない）——このまま詰めると、インストーラの中身と3バイト違う ZIP になります。pwsh tools/release.ps1 を使ってください"
}

# 版はtauri.conf.jsonを唯一の出どころにする（ここで二重管理しない）
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version

# 組み立て場も置き場ごと作り直す（ZIPの置き場と同じ理屈）。同じ版のぶんだけ
# 消していると、版を上げるたびに pictkura-0.1.0 / pictkura-0.1.1 と溜まっていく。
$stageRoot = Join-Path $releaseDir "portable"
if (Test-Path $stageRoot) { Remove-Item -Recurse -Force $stageRoot }
$stage = Join-Path $stageRoot "pictkura-$version"
New-Item -ItemType Directory -Path (Join-Path $stage "docs\images") -Force | Out-Null

Copy-Item $exe (Join-Path $stage "pictkura.exe")
Copy-Item (Join-Path $root "LICENSE") $stage
Copy-Item (Join-Path $root "THIRD-PARTY-LICENSES.txt") $stage
# 取扱説明書は**グロブで拾う**。1つずつ書いていると、言語を足したときに
# 黙って落ちる（実際に落とした——`tauri.conf.json` に足しただけでは
# ポータブルZIPに入らなかった）。CIも「リポジトリにある分が全部入っているか」
# を突き合わせる形にしてある
Copy-Item (Join-Path $root "docs\manual*.html") (Join-Path $stage "docs")
Copy-Item (Join-Path $root "docs\images\*.jpg") (Join-Path $stage "docs\images")

# 展開した人が最初に開くもの。ZIPを開いた画面で目に入る位置に置く
@"
pictkura $version（持ち歩き版）

インストールは要りません。pictkura.exe をそのまま実行してください。

- 使い方: docs\manual.html をブラウザで開いてください（英語版は docs\manual.en.html）
- ライセンス: LICENSE（MIT）
- 同梱しているオープンソース: THIRD-PARTY-LICENSES.txt

設定と写真の索引は次の場所に作られます（写真そのものは入りません）。
  %APPDATA%\dev.harusame.pictkura\

アンインストールは、このフォルダと上の設定フォルダを消すだけです。
ただし、USB/SDカードを挿したときに「pictkura で写真を取り込む」を候補として
出す設定（既定で有効）は、Windowsのレジストリに書かれています。**消す前に**
次を実行して解除してください（窓は出ません。すぐ終わります）。

  pictkura.exe --unregister-autoplay

解除しないまま消すと、カードを挿すたびに、もう無い pictkura を呼ぶ候補が
並び続けます。
"@ | Set-Content -Path (Join-Path $stage "はじめにお読みください.txt") -Encoding utf8

# **置き場ごと作り直す**。同じ版のZIPを消すだけだと、版を上げた最初の実行で
# 前の版のZIPが隣に残り、CIの員数確認（ポータブルZIPは1つ）が落ちる。
$out = Join-Path $releaseDir "bundle\portable"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out -Force | Out-Null
$zip = Join-Path $out "pictkura_${version}_x64-portable.zip"
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Output "作成: $zip  ($size MB)"
