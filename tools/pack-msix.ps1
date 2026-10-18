# Microsoft Store 版（MSIX）を包む（pictkura-dev #53。計画は dev/plan.msix.md）。
#
#   pwsh tools/release.ps1          # 先に。MSI・NSIS・持ち歩き版と同じバイトの exe を作る
#   pwsh tools/pack-msix.ps1        # 試験用の Identity で未署名の MSIX
#   pwsh tools/pack-msix.ps1 -TestSign   # ＋自己署名した MSIX と、入れるための .cer
#
# **exe は作らない。** release.ps1 の exe（目印を伏せた、3つの入れ物と同じバイト）をそのまま包む——ここで
# 作り直すと、Store 版だけ中身の違う exe になる。
#
# Identity の3つは Partner Center で名前を予約すると決まる。それまでは試験用の値（引数で差し替える）。
# **Store へ出すのは未署名のほう**（Store が署名する）。自己署名は手元で入れて確かめるためだけ。
param(
    [string]$IdentityName = "Harusame64.pictkura.test",
    [string]$Publisher = "CN=pictkura-test",
    [string]$PublisherDisplayName = "Harusame64",
    [switch]$TestSign
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "bundle-token.ps1")

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $targetDir = (cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json).target_directory
    if ($LASTEXITCODE -ne 0 -or -not $targetDir) { throw "cargo metadata から置き場を取れない" }
} finally { Pop-Location }

if ($env:CARGO_BUILD_TARGET) {
    throw "CARGO_BUILD_TARGET が設定されている（$env:CARGO_BUILD_TARGET）——cargo と cargo tauri bundle が別の置き場を見るので、この工程は支えていません"
}
$releaseDir = Join-Path $targetDir "release"
$exe = Join-Path $releaseDir "pictkura.exe"
# 持ち歩き版と同じ守り（pack-portable.ps1）: 実行ファイルが1か所だけで、それが target\release にあること——
# 三つ組つきの置き場に新しい exe があるのに、前の回の target\release を黙って包まない（ゲート2）
$builds = @(Get-ReleaseBinaryCandidates $targetDir "pictkura.exe")
if ($builds.Count -eq 0) { throw "実行ファイルがない: $exe （先に pwsh tools/release.ps1 を実行してください）" }
if ($builds.Count -gt 1) { throw "実行ファイルが $($builds.Count) 箇所にある——どれを包むのか決められない: $($builds.FullName -join ', ')" }
if ($builds[0].FullName -ne $exe) { throw "実行ファイルの置き場が想定と違う: $($builds[0].FullName)" }
# 持ち歩き版と同じ守り: release.ps1 を通った exe か（目印が伏せてあるか）
$blob = [IO.File]::ReadAllBytes($exe)
$found = Test-BundleToken $blob $BundleTokenReplacement
$blob = $null
if (-not $found) {
    throw "この実行ファイルは release.ps1 を通っていない（目印が入っていない）——このまま包むと、インストーラの中身と違う exe の MSIX になります"
}

# 版は4つの数（4つ目は Store 用に 0 固定）。Store は1つ目の 0 を受け付けない（1.0.0 からは合う）
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "版が3つの数ではない: $version" }
$msixVersion = "$version.0"
if ($version.StartsWith("0.")) {
    Write-Warning "版 $msixVersion は1つ目が 0——試験には包めるが、Store は受け付けない"
}

# 並べる（持ち歩き版と同じ中身。about の文書は exe の隣の docs\ から読む）
$stage = Join-Path $releaseDir "msix\layout"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path (Join-Path $stage "docs\images"), (Join-Path $stage "Assets") -Force | Out-Null
Copy-Item $exe (Join-Path $stage "pictkura.exe")
Copy-Item (Join-Path $root "LICENSE") $stage
Copy-Item (Join-Path $root "THIRD-PARTY-LICENSES.txt") $stage
Copy-Item (Join-Path $root "docs\manual*.html") (Join-Path $stage "docs")
Copy-Item (Join-Path $root "docs\images\*.jpg") (Join-Path $stage "docs\images")

# ロゴ（アプリのアイコンから縮める）。マニフェストが名指す3つ
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Image]::FromFile((Join-Path $root "src-tauri\icons\icon.png"))
try {
    foreach ($a in @(@{ n = "Square44x44Logo.png"; s = 44 }, @{ n = "Square150x150Logo.png"; s = 150 }, @{ n = "StoreLogo.png"; s = 50 })) {
        $bmp = New-Object System.Drawing.Bitmap $a.s, $a.s
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($src, 0, 0, $a.s, $a.s)
        $g.Dispose()
        $bmp.Save((Join-Path $stage "Assets\$($a.n)"), [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
    }
} finally { $src.Dispose() }

# マニフェストを埋める（XML として埋める値はエスケープする）
$esc = { param($v) [System.Security.SecurityElement]::Escape($v) }
$manifest = Get-Content (Join-Path $root "src-tauri\windows\msix\AppxManifest.xml") -Raw
$manifest = $manifest.Replace("{{IDENTITY_NAME}}", (& $esc $IdentityName)).
    Replace("{{PUBLISHER}}", (& $esc $Publisher)).
    Replace("{{PUBLISHER_DISPLAY_NAME}}", (& $esc $PublisherDisplayName)).
    Replace("{{VERSION}}", $msixVersion)
if ($manifest -match '\{\{[A-Z_]+\}\}') { throw "マニフェストに埋めていない値が残っている: $($Matches[0])" }
Set-Content -Path (Join-Path $stage "AppxManifest.xml") -Value $manifest -Encoding utf8

# Windows SDK の makeappx（いちばん新しい版の x64）
$kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
$makeappx = Get-ChildItem $kits -Recurse -Filter makeappx.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw "makeappx.exe が見つからない（Windows SDK が要る）: $kits" }

$out = Join-Path $releaseDir "bundle\msix"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out -Force | Out-Null
$msix = Join-Path $out "pictkura_${version}_x64.msix"
& $makeappx.FullName pack /d $stage /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx が $LASTEXITCODE で落ちた" }
Write-Output "作成: $msix  ($([math]::Round((Get-Item $msix).Length / 1MB, 1)) MB)（未署名。Store へ出すのはこちら）"

if ($TestSign) {
    # 試験用の自己署名。Subject はマニフェストの Publisher と同じでないと署名が通らない。
    # 入れる側は .cer を「信頼されたユーザー」へ入れるか、開発者モードで入れる（どちらも台の設定を変える）
    $signtool = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $signtool) { throw "signtool.exe が見つからない" }
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
        -FriendlyName "pictkura MSIX test" -CertStoreLocation "Cert:\CurrentUser\My" `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    try {
        $pfxPass = ConvertTo-SecureString -String "pictkura-test" -Force -AsPlainText
        $pfx = Join-Path $out "pictkura-test.pfx"
        Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pfxPass | Out-Null
        Export-Certificate -Cert $cert -FilePath (Join-Path $out "pictkura-test.cer") | Out-Null
        $signed = Join-Path $out "pictkura_${version}_x64_test-signed.msix"
        Copy-Item $msix $signed
        & $signtool.FullName sign /fd SHA256 /a /f $pfx /p "pictkura-test" $signed
        if ($LASTEXITCODE -ne 0) { throw "signtool が $LASTEXITCODE で落ちた" }
        Remove-Item $pfx
        Write-Output "作成: $signed（試験用の自己署名）と pictkura-test.cer"
    } finally {
        Remove-Item "Cert:\CurrentUser\My\$($cert.Thumbprint)" -ErrorAction SilentlyContinue
    }
}
