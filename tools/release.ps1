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
#
# **ビルドと束ねるのを2段に割ってあるのは、3つの入れ物に同じバイト列を入れるため。**
# 詳しくは 3/4 の節に書いた。
$ErrorActionPreference = "Stop"

# 目印の語と、バイト列を探す道具は **pack-portable.ps1 と共有する**
# （別々に書くと、片方だけ変えたときに正しいビルドが弾かれる）。
. (Join-Path $PSScriptRoot "bundle-token.ps1")

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    # **置き場を cargo に訊く。** `CARGO_TARGET_DIR` が環境に在ると、
    # `target\release` を決め打ちした側だけ**別の場所を見る**——本体は新しい置き場に
    # できるのに、**こちらは古い実行ファイルを潰して、束ねるのは潰していないほう**になる。
    # **成功したと言いながら3本のバイト列が出る**、というのが一番まずい落ち方なので、
    # 決め打ちをやめる。CI はこの変数を置かないので、そこでは同じ値になる。
    $targetDir = (cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json).target_directory
    if ($LASTEXITCODE -ne 0 -or -not $targetDir) { throw "cargo metadata から置き場を取れない" }
    # **三つ組（target triple）を指定した状態では走らない。**
    # `CARGO_BUILD_TARGET` は **cargo は読むが `cargo tauri bundle` は読まない**
    # （tauri-cli は `.cargo/config.toml` の `build.target` と `$CARGO_HOME/config` しか見ない）。
    # つまりこの変数があると、**本体は `target\<triple>\release` にでき、
    # 束ねるほうは `target\release` を見る**——**古い実行ファイルが残っていれば、
    # 成功したと言いながら潰していないほうが配布物に入る。**
    # **道具どうしが食い違っている経路なので、支えるふりをせず断る。**
    if ($env:CARGO_BUILD_TARGET) {
        throw "CARGO_BUILD_TARGET が設定されている（$env:CARGO_BUILD_TARGET）。cargo は target\<triple>\release に作りますが、cargo tauri bundle はこの変数を読まず target\release を見ます——食い違ったまま配布物ができるので、変数を外して走らせてください"
    }
    $releaseDir = Join-Path $targetDir "release"
    Write-Output "== 1/4 UI をビルド =="
    npm --prefix ui run build
    if ($LASTEXITCODE -ne 0) { throw "UIのビルドに失敗" }

    # **前の版のインストーラを置き場ごと片付ける**。`cargo tauri build` は自分の版の
    # ファイルしか書かないので、版を上げた最初の実行では 0.1.0 のものが
    # 0.1.1 の隣に残り、CIの員数確認（MSIが2つ・NSISが1つ・ZIPが1つ）が落ちる。
    # ポータブルZIPの置き場は pack-portable.ps1 が同じことをする。
    # macOS側の release-macos.sh も置き場ごと作り直している。
    foreach ($name in "msi", "nsis") {
        $dir = Join-Path $releaseDir "bundle\$name"
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    }

    $exe = Join-Path $releaseDir "pictkura.exe"

    Write-Output "== 2/4 本体をビルド（まだ束ねない） =="
    Push-Location (Join-Path $root "src-tauri")
    try {
        cargo tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "cargo tauri build --no-bundle に失敗" }
    } finally { Pop-Location }

    # **入れ物ごとの書き換えを、先回りで止める。**
    #
    # `tauri-bundler` は入れ物ごとに実行ファイルの中の `__TAURI_BUNDLE_TYPE_VAR_UNK` を
    # `..._NSS` / `..._MSI` へ書き換える。**3バイトしか違わないが、Defender の判定は
    # バイト列1本ごとに付く**ので、**同じビルドから「くじ」が3本できる**。
    # 0.2.7 で「1本取り下げれば済む」と読んで失敗したのはこれで、
    # **取り下げられたのは利用者が走らせない側だった。**
    #
    # 語が見つからなければ bundler は**警告を出して、そのまま束ねる**
    # （`tauri-bundler` の `bundle_project`。`patch_binary` の `Err` は握り潰される）。
    # だから**ここで語を潰しておけば、NSIS・MSI・ポータブルの3つに同じバイト列が入る。**
    #
    # アプリは `bundle_type` を一度も読んでいないし、updater プラグインも入れていないので、
    # **この語が何であっても動きは変わらない**。
    #
    # **書き込みは `target\release\deps\pictkura-<hash>.exe` にも届く**——cargo は
    # `target\release\pictkura.exe` を**ハードリンクで持ち上げている**ので、実体は1つで、
    # ここを truncate すると両方が変わる。**cargo はそれを新しいと見なす**ので、
    # `..._UNK` は `cargo clean` するまで戻らない。**副作用が1つある**:
    # **このあと素の `cargo tauri build` を回すと、bundler が
    # 「`__TAURI_BUNDLE_TYPE variable not found in binary. Make sure tauri crate and
    # tauri-cli are up to date`」と警告する。** 版の食い違いに見えるが**そうではない**
    # ——この工程が潰した跡である。**戻したいなら `cargo clean -p pictkura`。**
    Write-Output "== 3/4 入れ物ごとの書き換えを止める =="
    # **三つ組が効いていないかを、設定ではなく置き場の形で確かめる。**
    # `build.target`（`.cargo/config.toml` か CARGO_HOME の config）が効いていると、
    # cargo も tauri-cli も `target\<triple>\release` を使う。ところが
    # `cargo metadata` の `target_directory` は**三つ組に関係なく `target`** なので、
    # `$exe` は `target\release\pictkura.exe` を指したままになる。
    # **そこに昔の実行ファイルが残っていると、`Test-Path` は通ってしまう**
    # ——**古い本体を潰して、束ねるのは潰していない新しいほう**、という
    # **成功したと言いながら3本出す**形になる。設定を読みに行くより、
    # **`*/release/pictkura.exe` を数えるほうが確実**である。
    $builds = @(Get-ReleaseBinaryCandidates $targetDir "pictkura.exe")
    if ($builds.Count -eq 0) {
        throw "実行ファイルがない（$targetDir の下に */release/pictkura.exe が1つも無い）"
    }
    if ($builds.Count -gt 1) {
        throw "実行ファイルが $($builds.Count) 箇所にある——どれを配るのか決められない: $($builds.FullName -join ', ')。三つ組を指定した置き場が混ざっているなら、片方を消してから走らせてほしい"
    }
    if ($builds[0].FullName -ne $exe) {
        throw "実行ファイルの置き場が想定と違う: $($builds[0].FullName)。build.target で三つ組を指定していると target\<triple>\release になります——この工程はその形を支えていません（設定を外して走らせてください）"
    }
    $bytes = [IO.File]::ReadAllBytes($exe)

    # **全件置き換える。** bundler は最初に見つけた1つしか書き換えないので、
    # **1つでも潰し残すとそちらが書き換えられて、また3本に割れる。**
    # 「1件のはず」で当てない。
    $hits = Replace-AllBytes $bytes $BundleToken $BundleTokenReplacement
    if ($hits -gt 0) {
        Write-Output "  $BundleToken を $hits 件つぶした（-> $BundleTokenReplacement）"
    } elseif (Test-BundleToken $bytes $BundleTokenReplacement) {
        # **先に「もう済んでいる」を見る。** ここを後ろに置くと、
        # **前回この処理を通した本体で走らせ直しただけ**なのに、下の生存判定に引っかかって
        # 「作り直してほしい」と言ってしまう（`bundle_type()` が生きている世界で顕在化する）。
        Write-Output "  $BundleTokenReplacement が既に入っている（前回の実行で潰した本体をそのまま使う）"
    } else {
        # **`..._UNK` も `..._PKR` も無い。** 残るのは2つ——**前回の束ねが途中で落ちて
        # `..._NSS` / `..._MSI` が入ったまま**か、**上流が語そのものを変えた**か。
        #
        # ただし `..._NSS` と `..._MSI` は **`bundle_type()` の照合先の文字列でもある。**
        # いまの実行ファイルに入っていないのは**あの関数が誰にも呼ばれず落とされている**
        # からで（実測: `..._UNK` が1件、`..._NSS`/`..._MSI` は0件）、**updater を入れれば
        # 生き返る**。生きているときに掃くと、**照合先ごと潰して `bundle_type()` を壊し**、
        # しかも毎回「前回の束ねが落ちた」と嘘をつく。
        #
        # **生きているかは `..._DEB` / `..._RPM` / `..._APP` で分かる**——あの関数の
        # 照合先にそのまま入っていて、**Windows 向けの実行ファイルには他に出どころが無い。**
        foreach ($live in $BundleTypeArmsThatProveLiveness) {
            if (Test-BundleToken $bytes $live) {
                throw "$live が入っている＝bundle_type() が生きている実行ファイルなので、前回の残り物と照合先の文字列を見分けられない。本体を作り直してほしい（cargo clean -p pictkura）"
            }
        }

        # **1件のときだけ残り物と見なす。** 2件以上なら静的な文字列を巻き込んでいる。
        foreach ($name in $BundleTokenLeftovers) {
            $count = Measure-BundleToken $bytes $name
            if ($count -eq 0) { continue }
            if ($count -gt 1) {
                throw "$name が $count 件ある。前回の束ねの残り物ではなく、実行ファイルの中の文字列を巻き込む——本体を作り直してほしい（cargo clean -p pictkura）"
            }
            $hits += Replace-AllBytes $bytes $name $BundleTokenReplacement
            Write-Output "  $name が1件入っていた——前回の束ねが途中で落ちた本体をそのまま使う（-> $BundleTokenReplacement）"
            # **見つけた1つで打ち切らない。** いまの bundler は1箇所しか書き換えないので
            # 2種類が同時に入ることは無いが、**打ち切ると残ったほうが生き残り**、
            # ポータブル側の確認（`..._PKR` が在るか）は通ってしまう。
            # **他の枝はどこも全件やっているので、ここだけ緩めない。**
        }

        # **目印が1つも無い。** 黙って通すと、この工事は何もしないまま3本に戻る。
        if ($hits -eq 0) {
            throw "実行ファイルに目印が1つも無い（$BundleToken / $($BundleTokenLeftovers -join ' / ') / $BundleTokenReplacement）。tauri 側が語を変えた可能性がある——変わっていれば、この工事は効かないまま3本のバイト列が出る"
        }
    }

    if ($hits -gt 0) {
        [IO.File]::WriteAllBytes($exe, $bytes)
    }

    Write-Output "== 4/4 インストーラ（MSI・NSIS）と持ち歩き版を作る =="
    Push-Location (Join-Path $root "src-tauri")
    try {
        cargo tauri bundle
        if ($LASTEXITCODE -ne 0) { throw "cargo tauri bundle に失敗" }
    } finally { Pop-Location }

    & (Join-Path $PSScriptRoot "pack-portable.ps1")

    Write-Output ""
    Write-Output "できたもの:"
    Get-ChildItem -Recurse -File (Join-Path $releaseDir "bundle") |
        ForEach-Object { "  {0,7:N1} MB  {1}" -f ($_.Length / 1MB), $_.Name }
} finally { Pop-Location }
