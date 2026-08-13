# インストール不要で動く「持ち歩き版」のZIPを作る。
#
#   cargo tauri build    # 先に本体とMSIを作っておく（src-tauri で実行）
#   pwsh tools/pack-portable.ps1
#
# MSIと同じものを、レジストリにもスタートメニューにも触らない形で配る。
# 中身の並びは**インストール後と同じ**にする——アプリは実行ファイルの隣から
# 取扱説明書とライセンス一覧を探すので、ここを崩すと「同梱されていません」になる。
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\release\pictkura.exe"
if (-not (Test-Path $exe)) {
    throw "実行ファイルがない: $exe （先に cargo tauri build を実行してください）"
}

# 版はtauri.conf.jsonを唯一の出どころにする（ここで二重管理しない）
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version

$stage = Join-Path $root "target\release\portable\pictkura-$version"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path (Join-Path $stage "docs\images") -Force | Out-Null

Copy-Item $exe (Join-Path $stage "pictkura.exe")
Copy-Item (Join-Path $root "LICENSE") $stage
Copy-Item (Join-Path $root "THIRD-PARTY-LICENSES.txt") $stage
Copy-Item (Join-Path $root "docs\manual.html") (Join-Path $stage "docs")
Copy-Item (Join-Path $root "docs\images\*.jpg") (Join-Path $stage "docs\images")

# 展開した人が最初に開くもの。ZIPを開いた画面で目に入る位置に置く
@"
pictkura $version（持ち歩き版）

インストールは要りません。pictkura.exe をそのまま実行してください。

- 使い方: docs\manual.html をブラウザで開いてください
- ライセンス: LICENSE（MIT）
- 同梱しているオープンソース: THIRD-PARTY-LICENSES.txt

設定と写真の索引は次の場所に作られます（写真そのものは入りません）。
  %APPDATA%\dev.harusame.pictkura\

アンインストールは、このフォルダと上の設定フォルダを消すだけです。
"@ | Set-Content -Path (Join-Path $stage "はじめにお読みください.txt") -Encoding utf8

$zip = Join-Path $root "target\release\bundle\portable\pictkura_${version}_x64-portable.zip"
New-Item -ItemType Directory -Path (Split-Path $zip) -Force | Out-Null
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Output "作成: $zip  ($size MB)"
