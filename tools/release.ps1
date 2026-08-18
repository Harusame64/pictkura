# 配布物を一式まとめて作る。
#
#   pwsh tools/release.ps1
#
# **UIのビルドを先に必ず走らせる**のが要点。`ui/dist` は生成物でリポジトリに
# 入っていないため、これを飛ばすと「ディスクにたまたま残っていた古いdist」が
# そのままMSIとZIPへ入り、しかも何のエラーも出ない。
#
# Tauri の `beforeBuildCommand` でも同じことはできるが、実行時の作業ディレクトリが
# 呼び出し方で変わって当てにならなかったので、手順ごとここへ置く。
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Write-Output "== 1/3 UI をビルド =="
    npm --prefix ui run build
    if ($LASTEXITCODE -ne 0) { throw "UIのビルドに失敗" }

    # **前の版のインストーラを置き場ごと片付ける**。`cargo tauri build` は自分の版の
    # ファイルしか書かないので、版を上げた最初の実行では 0.1.0 のものが
    # 0.1.1 の隣に残り、CIの員数確認（MSIが2つ・NSISが1つ・ZIPが1つ）が落ちる。
    # ポータブルZIPの置き場は pack-portable.ps1 が同じことをする。
    # macOS側の release-macos.sh も置き場ごと作り直している。
    foreach ($name in "msi", "nsis") {
        $dir = Join-Path $root "target\release\bundle\$name"
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    }

    Write-Output "== 2/3 本体とインストーラ（MSI・NSIS）をビルド =="
    Push-Location (Join-Path $root "src-tauri")
    try {
        cargo tauri build
        if ($LASTEXITCODE -ne 0) { throw "cargo tauri build に失敗" }
    } finally { Pop-Location }

    Write-Output "== 3/3 持ち歩き版のZIPを作る =="
    & (Join-Path $PSScriptRoot "pack-portable.ps1")

    Write-Output ""
    Write-Output "できたもの:"
    Get-ChildItem -Recurse -File (Join-Path $root "target\release\bundle") |
        ForEach-Object { "  {0,7:N1} MB  {1}" -f ($_.Length / 1MB), $_.Name }
} finally { Pop-Location }
