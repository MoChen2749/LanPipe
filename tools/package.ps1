<#
.SYNOPSIS
    一键打包 LanPipe，并把所有最终产物统一放到仓库根目录的 artifacts\ 文件夹。

.DESCRIPTION
    产物类型（-Bundles 选择，默认 msi,exe）：
      msi       Windows MSI 安装包 + 更新签名（.msi.sig）
      exe       Windows 独立可执行文件
      linux     Linux 桌面二进制 + 静态 CLI（Windows 上用 rust-lld 交叉编译 CLI，
                桌面端需要 WSL 环境，见 tools\build-linux.ps1）
      android   Android APK（arm64-v8a，debug 构建）
      portable  便携版 zip（调用 tools\make-portable.ps1，默认不构建）
      all       以上全部

    产物统一命名，放在 artifacts\ 下：
      LanPipe_<version>_x64_zh-CN.msi
      LanPipe_<version>_x64_zh-CN.msi.sig
      LanPipe_<version>_x64-windows.exe
      LanPipe_<version>_x86_64-linux
      lanpipe-cli_<version>_x86_64-linux
      LanPipe_<version>_arm64-android-debug.apk
      LanPipe_<version>_x64-portable.zip
      latest.json（-Publish 时）

    注意：本文件必须保存为 UTF-8 带 BOM，否则 Windows PowerShell 5.1 会把中文
    按 ANSI 解析而报错。

.PARAMETER Bundles
    产物类型，可写 msi、exe、linux、android、portable、all，逗号分隔。默认 msi,exe。

.PARAMETER SkipFrontend
    跳过 npm run build（确认 dist\ 已是最新前端产物时可加）。

.PARAMETER Version
    可选，仅用于核对；实际版本以 apps\desktop\src-tauri\tauri.conf.json 为准。

.PARAMETER OutDir
    产物输出目录，默认 artifacts（仓库根目录下）。

.PARAMETER Clean
    构建前清空输出目录中旧的 LanPipe_* / lanpipe-cli_* / latest.json。

.PARAMETER Publish
    打包完成后生成自动更新清单 latest.json（需要 MSI）。

.PARAMETER Url
    MSI 下载地址，写入 latest.json；未指定时读取环境变量 LANPIPE_RELEASE_URL。

.PARAMETER Notes
    可选，写入 latest.json 的更新说明。

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1 -Bundles all -Clean

.EXAMPLE
    $env:LANPIPE_RELEASE_URL = 'https://example.com/lanpipe/LanPipe_0.1.2_x64_zh-CN.msi'
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'lanpipe-dev-key'
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1 -Publish
#>
[CmdletBinding()]
param(
    [string[]]$Bundles = @('msi', 'exe'),
    [switch]$SkipFrontend,
    [string]$Version = '',
    [string]$OutDir = 'artifacts',
    [switch]$Clean,
    [switch]$Publish,
    [string]$Url = '',
    [string]$Notes = ''
)

$ErrorActionPreference = 'Stop'
trap {
    Write-Host ''
    Write-Host ('[失败] ' + $_.Exception.Message) -ForegroundColor Red
    if ($_.InvocationInfo -and $_.InvocationInfo.ScriptLineNumber) {
        Write-Host ('       位置：{0} 第 {1} 行' -f $_.InvocationInfo.ScriptName, $_.InvocationInfo.ScriptLineNumber) -ForegroundColor DarkGray
    }
    exit 1
}
try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false) } catch { }
$OutputEncoding = New-Object System.Text.UTF8Encoding($false)

$root = Split-Path -Parent $PSScriptRoot
$bundleRoot = Join-Path $root 'target\release\bundle'
$psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
if (-not (Test-Path -LiteralPath $psExe)) { $psExe = 'powershell' }
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

function Write-Title([string]$text) {
    Write-Host ''
    Write-Host ('==> ' + $text) -ForegroundColor Cyan
}
function Write-Detail([string]$text) { Write-Host ('    ' + $text) -ForegroundColor Gray }
function Write-Notice([string]$text) { Write-Host ('[提示] ' + $text) -ForegroundColor Yellow }
function Write-Done([string]$text) { Write-Host ('[完成] ' + $text) -ForegroundColor Green }

function Assert-ExitCode([string]$what) {
    if ($LASTEXITCODE -ne 0) { throw ('{0} 失败（退出码 {1}），请查看上方日志。' -f $what, $LASTEXITCODE) }
}

function Resolve-RepoPath([string]$path) {
    if ([System.IO.Path]::IsPathRooted($path)) { return [System.IO.Path]::GetFullPath($path) }
    return [System.IO.Path]::GetFullPath((Join-Path $root $path))
}

function Get-ToolPath([string]$name, [string]$hint, [string]$fallbackDir, [string]$fallbackExe) {
    $cmd = Get-Command $name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    if ($fallbackDir -and $fallbackExe) {
        $candidate = Join-Path $fallbackDir $fallbackExe
        if (Test-Path -LiteralPath $candidate) {
            if (-not (($env:PATH -split ';') -contains $fallbackDir)) {
                $env:PATH = $fallbackDir + ';' + $env:PATH
                Write-Notice ('{0} 不在 PATH，本次运行已临时加入 {1}；建议永久加入 PATH。' -f $name, $fallbackDir)
            }
            return $candidate
        }
    }
    throw ('未找到 {0}。{1}' -f $name, $hint)
}

# 把一个文件复制到产物目录并统一改名；返回复制后的 FileInfo。
function Publish-Artifact([string]$source, [string]$name) {
    if (-not (Test-Path -LiteralPath $source)) { throw ('未找到待发布的产物：' + $source) }
    $target = Join-Path $artifactDir $name
    Copy-Item -LiteralPath $source -Destination $target -Force
    Write-Detail ('产物：' + $target)
    return (Get-Item -LiteralPath $target)
}

# 在目录里取最新修改的匹配文件。
function Get-LatestFile([string]$dir, [string]$filter) {
    if (-not (Test-Path -LiteralPath $dir)) { return $null }
    return (Get-ChildItem -LiteralPath $dir -Filter $filter -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1)
}

# ---------------------------------------------------------------- 参数规范化
$wanted = New-Object 'System.Collections.Generic.List[string]'
$validBundles = @('msi', 'exe', 'linux', 'android', 'portable')
foreach ($entry in $Bundles) {
    foreach ($piece in ($entry -split '[,;\s]+')) {
        $name = $piece.Trim().ToLowerInvariant()
        if ($name -eq '') { continue }
        if ($name -eq 'all') {
            foreach ($item in $validBundles) { if (-not $wanted.Contains($item)) { $wanted.Add($item) } }
            continue
        }
        if ($validBundles -notcontains $name) {
            throw ('未知的 -Bundles 取值：{0}（可选：{1}，或 all）' -f $piece, ($validBundles -join '、'))
        }
        if (-not $wanted.Contains($name)) { $wanted.Add($name) }
    }
}
if ($wanted.Count -eq 0) { throw '请至少指定一种产物类型，例如 -Bundles msi,exe' }
$wantMsi = $wanted.Contains('msi')
$wantExe = $wanted.Contains('exe')
$wantLinux = $wanted.Contains('linux')
$wantAndroid = $wanted.Contains('android')
$wantPortable = $wanted.Contains('portable')
$needFrontend = $wantMsi -or $wantExe -or $wantLinux -or $wantAndroid -or $wantPortable
$bundleNames = ($wanted -join ', ')

if ($Publish -and -not $wantMsi) {
    throw '生成 latest.json 需要 MSI：请把 msi 加入 -Bundles。'
}

$artifactDir = Resolve-RepoPath $OutDir
New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null
if ($Clean) {
    Get-ChildItem -LiteralPath $artifactDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like 'LanPipe_*' -or $_.Name -like 'lanpipe-cli_*' -or $_.Name -eq 'latest.json' } |
        Remove-Item -Force
}

# ---------------------------------------------------------------- 前置检查
Write-Host ''
Write-Host '============================================================' -ForegroundColor Green
Write-Host '  LanPipe 打包工具' -ForegroundColor Green
Write-Host ('  仓库：' + $root) -ForegroundColor Green
Write-Host ('  产物类型：' + $bundleNames) -ForegroundColor Green
Write-Host ('  产物目录：' + $artifactDir) -ForegroundColor Green
Write-Host '============================================================' -ForegroundColor Green

Write-Title '前置检查'

$tauriConfPath = Join-Path $root 'apps\desktop\src-tauri\tauri.conf.json'
if (-not (Test-Path -LiteralPath $tauriConfPath)) {
    throw ('未找到 {0}，请确认在 LanPipe 仓库根目录下运行本脚本。' -f $tauriConfPath)
}
$realVersion = ''
try {
    $confText = [System.IO.File]::ReadAllText($tauriConfPath, [System.Text.Encoding]::UTF8)
    $conf = $confText | ConvertFrom-Json
    if ($conf -and $conf.version) { $realVersion = [string]$conf.version }
} catch {
    throw ('读取 tauri.conf.json 失败：' + $_.Exception.Message)
}
if (-not $realVersion) {
    $pkgJsonPath = Join-Path $root 'package.json'
    if (Test-Path -LiteralPath $pkgJsonPath) {
        $pkgText = [System.IO.File]::ReadAllText($pkgJsonPath, [System.Text.Encoding]::UTF8)
        $realVersion = [string](($pkgText | ConvertFrom-Json).version)
    }
}
if (-not $realVersion) { $realVersion = '0.0.0' }
Write-Detail ('版本：' + $realVersion)
if ($Version -and $Version.Trim() -ne '' -and $Version.Trim() -ne $realVersion) {
    Write-Notice ('指定版本 {0} 与配置版本 {1} 不一致；产物按配置版本命名。' -f $Version.Trim(), $realVersion)
}

$nodeExe = Get-ToolPath 'node' '请安装 Node.js 18+（https://nodejs.org/）并确认 node 在 PATH 中。' '' ''
$npmExe = Get-ToolPath 'npm' 'npm 通常随 Node.js 一起安装，请检查 Node.js 安装是否完整。' '' ''
$npxExe = Get-ToolPath 'npx' 'npx 通常随 Node.js 一起安装，请检查 Node.js 安装是否完整。' '' ''
$cargoDir = Join-Path $env:USERPROFILE '.cargo\bin'
$cargoExe = Get-ToolPath 'cargo' ('请安装 Rust（https://win.rustup.rs）或把 ' + $cargoDir + ' 加入 PATH 后重试。') $cargoDir 'cargo.exe'
Write-Detail ('node ' + ((& $nodeExe --version 2>&1 | Out-String).Trim()) + '，' + ((& $cargoExe --version 2>&1 | Out-String).Trim()))

$tauriCmd = Join-Path $root 'node_modules\.bin\tauri.cmd'
if (($wantMsi -or $wantAndroid) -and -not (Test-Path -LiteralPath $tauriCmd)) {
    throw '未找到 node_modules\.bin\tauri.cmd，请先在仓库根目录执行 npm install（需要 @tauri-apps/cli）。'
}
if ($wantLinux -and -not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'build-linux.ps1'))) {
    throw ('未找到 Linux 构建脚本：' + (Join-Path $PSScriptRoot 'build-linux.ps1'))
}
if ($wantPortable -and -not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'make-portable.ps1'))) {
    throw ('未找到便携版脚本：' + (Join-Path $PSScriptRoot 'make-portable.ps1'))
}

$publishUrl = ''
if ($Publish) {
    if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'publish-release.ps1'))) {
        throw ('未找到发布脚本：' + (Join-Path $PSScriptRoot 'publish-release.ps1'))
    }
    $publishUrl = $Url
    if (-not $publishUrl) { $publishUrl = $env:LANPIPE_RELEASE_URL }
    $publishUrl = ([string]$publishUrl).Trim()
    if (-not $publishUrl) { throw '缺少 MSI 下载地址：请加 -Url <地址>，或先设置环境变量 LANPIPE_RELEASE_URL。' }
    if ($publishUrl -notmatch '^https?://') { throw ('MSI 下载地址必须以 http:// 或 https:// 开头：' + $publishUrl) }
    Write-Detail ('发布地址：' + $publishUrl)
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        throw '缺少更新签名口令：请先设置环境变量 TAURI_SIGNING_PRIVATE_KEY_PASSWORD。'
    }
}
Write-Done '前置检查通过'

# ---------------------------------------------------------------- 前端构建
$distIndex = Join-Path $root 'dist\index.html'
if ($SkipFrontend) {
    Write-Title '跳过前端构建（-SkipFrontend）'
    if ($needFrontend -and -not (Test-Path -LiteralPath $distIndex)) {
        throw ('跳过前端构建，但 dist\index.html 不存在：请去掉 -SkipFrontend 或先执行 npm run build。')
    }
} else {
    Write-Title '构建前端（npm run build）'
    Push-Location $root
    try {
        & $npmExe run build
        Assert-ExitCode 'npm run build（前端构建）'
    } finally { Pop-Location }
    if (-not (Test-Path -LiteralPath $distIndex)) { throw '前端构建后仍未找到 dist\index.html，请检查 vite.config.ts 与构建日志。' }
    Write-Done '前端构建完成（dist\）'
}

# ---------------------------------------------------------------- Windows exe / MSI
$exeSource = Join-Path $root 'target\release\lanpipe.exe'
if ($wantExe -and -not $wantMsi) {
    Write-Title '构建 Windows 独立可执行文件（cargo build --release）'
    Push-Location $root
    try {
        & $cargoExe build --release --features custom-protocol -p lanpipe
        Assert-ExitCode 'cargo build --release --features custom-protocol -p lanpipe'
    } finally { Pop-Location }
}

if ($wantMsi) {
    Write-Title '构建 MSI 安装包（tauri build --bundles msi）'
    Write-Detail '首次打包会自动下载 WiX 到 %LOCALAPPDATA%\tauri，可能需要几分钟 ...'
    Push-Location (Join-Path $root 'apps\desktop\src-tauri')
    try {
        & $npxExe --no-install tauri build --bundles msi
        Assert-ExitCode 'npx --no-install tauri build --bundles msi'
    } finally { Pop-Location }
    Write-Done 'MSI 构建完成'
}

# ---------------------------------------------------------------- Linux
if ($wantLinux) {
    Write-Title '构建 Linux CLI（rust-lld + musl 交叉编译）'
    Push-Location $root
    try {
        & $psExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build-linux.ps1') -Command cli
        Assert-ExitCode 'tools\build-linux.ps1 -Command cli'
    } finally { Pop-Location }

    Write-Title '构建 Linux 桌面二进制（WSL 内编译，需几分钟）'
    Push-Location $root
    try {
        & $psExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build-linux.ps1') -Command desktop
        Assert-ExitCode 'tools\build-linux.ps1 -Command desktop'
    } finally { Pop-Location }
    Write-Done 'Linux 构建完成'
}

# ---------------------------------------------------------------- Android
if ($wantAndroid) {
    Write-Title '构建 Android APK（arm64-v8a，debug）'
    # Android SDK / NDK：优先用本机已装位置，避免依赖当前会话是否继承过环境变量。
    $sdkCandidates = @('F:\Android\android-sdk', (Join-Path $env:LOCALAPPDATA 'Android\Sdk'))
    $sdk = $sdkCandidates | Where-Object { $_ -and (Test-Path -LiteralPath (Join-Path $_ 'platform-tools')) } | Select-Object -First 1
    if ($sdk) {
        $env:ANDROID_HOME = $sdk
        $env:ANDROID_SDK_ROOT = $sdk
        $ndkDir = Join-Path $sdk 'ndk'
        if (Test-Path -LiteralPath $ndkDir) {
            $ndk = Get-ChildItem -LiteralPath $ndkDir -Directory | Sort-Object Name -Descending | Select-Object -First 1
            if ($ndk) { $env:NDK_HOME = $ndk.FullName }
        }
        Write-Detail ('Android SDK：' + $sdk)
    } else {
        throw '未找到 Android SDK：请安装后再用 -Bundles android。'
    }
    $javaHome = 'C:\Program Files\Java\jdk-17'
    if (Test-Path -LiteralPath $javaHome) { $env:JAVA_HOME = $javaHome }
    $buildRoot = 'F:\Projects\.lanpipe-build'
    if (Test-Path -LiteralPath $buildRoot) {
        $env:CARGO_HOME = Join-Path $buildRoot 'cargo-home'
        $env:GRADLE_USER_HOME = Join-Path $buildRoot 'gradle-home'
        $env:TEMP = Join-Path $buildRoot 'tmp'
        $env:TMP = Join-Path $buildRoot 'tmp'
        $env:ANDROID_USER_HOME = Join-Path $buildRoot 'android-home'
    }
    Push-Location $root
    try {
        & $npxExe --no-install tauri android build --apk --debug --target aarch64
        Assert-ExitCode 'npx --no-install tauri android build --apk --debug --target aarch64'
    } finally { Pop-Location }
    Write-Done 'Android APK 构建完成'
}

# ---------------------------------------------------------------- 收集产物
Write-Title '收集产物到 artifacts 目录'
$artifacts = New-Object 'System.Collections.Generic.List[object]'

if ($wantMsi) {
    $msi = $null
    foreach ($dir in @((Join-Path $bundleRoot 'msi'), (Join-Path $root 'apps\desktop\src-tauri\target\release\bundle\msi'))) {
        $found = Get-LatestFile $dir '*.msi'
        if ($found -and (-not $msi -or $found.LastWriteTime -gt $msi.LastWriteTime)) { $msi = $found }
    }
    if (-not $msi) { throw 'tauri build 已结束，但未找到 MSI 产物，请查看上方日志。' }
    $msiName = 'LanPipe_{0}_x64_zh-CN.msi' -f $realVersion
    $artifacts.Add([pscustomobject]@{ Kind = 'MSI 安装包'; File = (Publish-Artifact $msi.FullName $msiName) })
    $sigSource = $msi.FullName + '.sig'
    if (Test-Path -LiteralPath $sigSource) {
        $artifacts.Add([pscustomobject]@{ Kind = 'MSI 更新签名'; File = (Publish-Artifact $sigSource ($msiName + '.sig')) })
    }
}

if ($wantExe) {
    if (-not (Test-Path -LiteralPath $exeSource)) {
        throw ('未找到 {0}，请确认已执行 Windows release 构建。' -f $exeSource)
    }
    $artifacts.Add([pscustomobject]@{ Kind = 'Windows 独立 exe'; File = (Publish-Artifact $exeSource ('LanPipe_{0}_x64-windows.exe' -f $realVersion)) })
}

if ($wantLinux) {
    $linuxDir = Join-Path $root 'dist-linux'
    $desktopBin = Join-Path $linuxDir 'lanpipe'
    $cliBin = Join-Path $linuxDir 'lanpipe-cli'
    if (-not (Test-Path -LiteralPath $cliBin)) { throw ('未找到 Linux CLI 产物：' + $cliBin) }
    $artifacts.Add([pscustomobject]@{ Kind = 'Linux CLI'; File = (Publish-Artifact $cliBin ('lanpipe-cli_{0}_x86_64-linux' -f $realVersion)) })
    if (Test-Path -LiteralPath $desktopBin) {
        $artifacts.Add([pscustomobject]@{ Kind = 'Linux 桌面二进制'; File = (Publish-Artifact $desktopBin ('LanPipe_{0}_x86_64-linux' -f $realVersion)) })
    } else {
        Write-Notice '未找到 Linux 桌面二进制（dist-linux\lanpipe），本次只发布 Linux CLI。'
    }
}

if ($wantAndroid) {
    $apkDirs = @(
        (Join-Path $root 'apps\desktop\src-tauri\gen\android\app\build\outputs\apk\universal\debug'),
        (Join-Path $root 'apps\desktop\src-tauri\gen\android\app\build\outputs\apk\aarch64\debug'),
        (Join-Path $root 'apps\desktop\src-tauri\gen\android\app\build\outputs\apk\release')
    )
    $apk = $null
    foreach ($dir in $apkDirs) {
        $found = Get-LatestFile $dir '*.apk'
        if ($found -and (-not $apk -or $found.LastWriteTime -gt $apk.LastWriteTime)) { $apk = $found }
    }
    if ($apk) {
        $artifacts.Add([pscustomobject]@{ Kind = 'Android APK'; File = (Publish-Artifact $apk.FullName ('LanPipe_{0}_arm64-android-debug.apk' -f $realVersion)) })
    } else {
        Write-Notice '未找到 Android APK 产物，跳过。'
    }
}

if ($wantPortable) {
    Write-Title '构建便携版 zip（tools\make-portable.ps1）'
    Push-Location $root
    try {
        & $psExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'make-portable.ps1')
        Assert-ExitCode 'tools\make-portable.ps1'
    } finally { Pop-Location }
    $zip = Get-LatestFile (Join-Path $bundleRoot 'portable') '*.zip'
    if ($zip) {
        $artifacts.Add([pscustomobject]@{ Kind = '便携版 zip'; File = (Publish-Artifact $zip.FullName ('LanPipe_{0}_x64-portable.zip' -f $realVersion)) })
    } else {
        Write-Notice '未找到便携版 zip，跳过。'
    }
}

# ---------------------------------------------------------------- latest.json
if ($Publish) {
    Write-Title '生成自动更新清单 latest.json'
    $msiArtifact = $artifacts | Where-Object { $_.Kind -eq 'MSI 安装包' } | Select-Object -First 1
    if (-not $msiArtifact) { throw '生成 latest.json 需要 MSI 产物，但本次没有 MSI。' }
    $latestJson = Join-Path $artifactDir 'latest.json'
    $publishArgs = @{
        Installer = $msiArtifact.File.FullName
        Version   = $realVersion
        Url       = $publishUrl
        OutFile   = $latestJson
    }
    if ($Notes -and $Notes.Trim() -ne '') { $publishArgs['Notes'] = $Notes }
    Push-Location $root
    try {
        & $psExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'publish-release.ps1') @publishArgs
        Assert-ExitCode 'tools\publish-release.ps1'
    } finally { Pop-Location }
    if (-not (Test-Path -LiteralPath $latestJson)) { throw ('未生成 latest.json：' + $latestJson) }
    $artifacts.Add([pscustomobject]@{ Kind = '更新清单'; File = (Get-Item -LiteralPath $latestJson) })
    Write-Done 'latest.json 生成完成'
}

# ---------------------------------------------------------------- 产物汇总
Write-Host ''
Write-Host '================ 产物汇总 ================' -ForegroundColor Green
Write-Host ('目录：' + $artifactDir) -ForegroundColor Green
foreach ($artifact in $artifacts) {
    $file = $artifact.File
    $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    Write-Host ('[{0}] {1}' -f $artifact.Kind, $file.Name) -ForegroundColor White
    Write-Host ('       大小：{0:N2} MB（{1:N0} 字节）' -f ($file.Length / 1MB), $file.Length)
    Write-Host ('       SHA256：{0}' -f $hash) -ForegroundColor Yellow
}
Write-Host ''

$elapsed = [int]$stopwatch.Elapsed.TotalSeconds
Write-Done ('打包完成，总耗时 {0} 秒。' -f $elapsed)
exit 0
