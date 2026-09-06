# 入れ物ごとの書き換えを止めるための語。**release.ps1 と pack-portable.ps1 で共有する。**
#
# 別々に書くと、**片方だけ変えたときに、正しいビルドが「release.ps1 を通っていない」と
# 弾かれる**。この語は「配るバイト列を、撃たれた文字列から動かす」ために在るので、
# **変えることが将来ふつうに起きる**——だから1箇所に置く。
#
# `tauri-bundler` が実行ファイルの中から探して書き換える語。**27バイト。**
# 見つかると入れ物ごとに `..._NSS` / `..._MSI` へ書き換えられ、**3バイト違いの
# 兄弟が3本できる**（ポータブルは束ね終わりに原本へ戻されるので `..._UNK` のまま）。
$script:BundleToken = "__TAURI_BUNDLE_TYPE_VAR_UNK"

# **前の実行が束ねの途中で落ちると、書き換えたままの本体がディスクに残る。**
# bundler は入れ物ごとに「書き換える → 束ねる → 原本へ戻す」の順で回るので、
# **束ねが転ぶと戻す手前で抜ける**（NSIS や WiX の取得が転ぶのがよくある形）。
# そのあと走らせ直しても cargo は本体を作り直さないので、**`..._UNK` は残っていない。**
$script:BundleTokenLeftovers = @("__TAURI_BUNDLE_TYPE_VAR_NSS", "__TAURI_BUNDLE_TYPE_VAR_MSI")

# 置き換える語。**同じ長さで、bundler が探す語とは違い、`bundle_type()` が知っている
# どの語とも違う**もの。`tauri-utils` の `bundle_type()` は知らない語なら `None` を返すので、
# **`..._UNK` のときと実行時の振る舞いは同じ**（`platform.rs` の `_ => None`）。
$script:BundleTokenReplacement = "__TAURI_BUNDLE_TYPE_VAR_PKR"

# `..._DEB` / `..._RPM` / `..._APP` は `bundle_type()` の照合先。**Windows 向けの
# 実行ファイルに他の出どころが無い**ので、在れば「あの関数が生きている」と読める。
$script:BundleTypeArmsThatProveLiveness = @(
    "__TAURI_BUNDLE_TYPE_VAR_DEB",
    "__TAURI_BUNDLE_TYPE_VAR_RPM",
    "__TAURI_BUNDLE_TYPE_VAR_APP"
)

# `$targetDir` の下で「配る実行ファイル」になりうる場所。**再帰しない。**
# 全部さらうと `target` は数十万ディレクトリになり、しかも読めない枝を
# 握り潰すと**2本目を見落として、この確認そのものが効かなくなる。**
function Get-ReleaseBinaryCandidates([string]$targetDir, [string]$name) {
    @(Get-ChildItem -Path (Join-Path $targetDir "release/$name") -File -ErrorAction Ignore) +
    @(Get-ChildItem -Path (Join-Path $targetDir "*/release/$name") -File -ErrorAction Ignore)
}

# バイト列の中から語を探す。**先頭バイトの一致だけ `IndexOf` に任せて飛ばす**
# ——26MB を1バイトずつ PowerShell で回すと、毎回のリリースに数分載る。
function Find-Bytes([byte[]]$hay, [byte[]]$needle, [int]$from) {
    $last = $hay.Length - $needle.Length
    $i = $from
    while ($i -le $last) {
        $i = [Array]::IndexOf($hay, $needle[0], $i, $last - $i + 1)
        if ($i -lt 0) { return -1 }
        $match = $true
        for ($j = 1; $j -lt $needle.Length; $j++) {
            if ($hay[$i + $j] -ne $needle[$j]) { $match = $false; break }
        }
        if ($match) { return $i }
        $i++
    }
    return -1
}

# 語（文字列）が在るか。
function Test-BundleToken([byte[]]$hay, [string]$text) {
    (Find-Bytes $hay ([Text.Encoding]::ASCII.GetBytes($text)) 0) -ge 0
}

# 語を全部置き換えて、置き換えた数を返す。**全件やる**——bundler は
# 最初に見つけた1つしか書き換えないので、**1つでも潰し残すとそちらが
# 書き換えられて、また3本に割れる。**
function Replace-AllBytes([byte[]]$hay, [string]$text, [string]$with) {
    $needle = [Text.Encoding]::ASCII.GetBytes($text)
    $repl = [Text.Encoding]::ASCII.GetBytes($with)
    if ($needle.Length -ne $repl.Length) { throw "置き換える語の長さが違う（PEを壊す）" }
    $n = 0
    $at = 0
    while (($at = Find-Bytes $hay $needle $at) -ge 0) {
        [Array]::Copy($repl, 0, $hay, $at, $repl.Length)
        $n++
        $at += $needle.Length
    }
    $n
}

# 語が何件あるか。
function Measure-BundleToken([byte[]]$hay, [string]$text) {
    $needle = [Text.Encoding]::ASCII.GetBytes($text)
    $n = 0
    $at = 0
    while (($at = Find-Bytes $hay $needle $at) -ge 0) { $n++; $at += $needle.Length }
    $n
}
