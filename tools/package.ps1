<#
.SYNOPSIS
    一键打包 LanPipe：MSI 安装包 + 便携版 zip，并汇总产物（路径 / 大小 / SHA256）。

.DESCRIPTION
    打包流程（用 -Bundles 选择要构建的产物类型）：
      1. 可选前端构建（npm run build，产出 dist\）
      2. MSI 安装包：在 apps\desktop\src-tauri 下执行
         npx --no-install tauri build --bundles msi
         （首次打包会自动下载 WiX 到 %LOCALAPPDATA%\tauri，比较慢）
      3. 便携版：调用 tools\make-portable.ps1 生成 zip
      4. 汇总产物（路径 / 大小 / SHA256）
      5. 可选：调用 tools\publish-release.ps1 生成 latest.json（-Publish）

    注意：本文件必须保存为 UTF-8 带 BOM，否则 Windows PowerShell 5.1 会把中文
    按 ANSI 解析而报错。不要用只写无 BOM 的编辑器保存。

.PARAMETER Bundles
    要构建的产物类型，可写 msi、portable（逗号分隔），默认两者都构建。

.PARAMETER SkipFrontend
    跳过 npm run build（确认 dist\ 下已有最新前端产物时可加）。

.PARAMETER Version
    可选，仅用于打印和核对版本号；实际版本以 apps\desktop\src-tauri\tauri.conf.json 为准。

.PARAMETER OutDir
    产物输出目录（相对仓库根或绝对路径），默认 target\release\bundle。
    指定其他目录时会按 msi\、portable\ 子目录复制产物过去。

.PARAMETER Publish
    打包完成后生成自动更新清单 latest.json（需要 MSI；会调用 tools\publish-release.ps1）。

.PARAMETER Url
    MSI 的下载地址，写入 latest.json；未指定时读取环境变量 LANPIPE_RELEASE_URL。

.PARAMETER Notes
    可选，写入 latest.json 的更新说明。

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1 -Bundles portable

.EXAMPLE
    $env:LANPIPE_RELEASE_URL = 'https://example.com/lanpipe/LanPipe_0.1.0_x64_zh-CN.msi'
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'lanpipe-dev-key'
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1 -Publish
#>
[CmdletBinding()]
param(
    [string[]]$Bundles = @('msi', 'portable'),
    [switch]$SkipFrontend,
    [string]$Version = '',
    [string]$OutDir = 'target\release\bundle',
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
# PowerShell 5.1 默认用 ANSI 输出；这里改成 UTF-8，保证中文与 cargo/npx 的 UTF-8 输出都不乱码。
# 没有控制台句柄（被重定向）时设置可能失败，忽略即可。
try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false) } catch { }
$OutputEncoding = New-Object System.Text.UTF8Encoding($false)

$root = Split-Path -Parent $PSScriptRoot
$bundleRoot = Join-Path $root 'target\release\bundle'
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

function Write-Title([string]$text) {
    Write-Host ''
    Write-Host ('==> ' + $text) -ForegroundColor Cyan
}

function Write-Detail([string]$text) {
    Write-Host ('    ' + $text) -ForegroundColor Gray
}

function Write-Notice([string]$text) {
    Write-Host ('[提示] ' + $text) -ForegroundColor Yellow
}

function Write-Done([string]$text) {
    Write-Host ('[完成] ' + $text) -ForegroundColor Green
}

function Assert-ExitCode([string]$what) {
    if ($LASTEXITCODE -ne 0) {
        throw ('{0} 失败（退出码 {1}），请查看上方日志。' -f $what, $LASTEXITCODE)
    }
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

# 产物在 OutDir 下的目标路径：默认 bundle 目录下保持 msi\ / portable\ 结构。
function Get-OutDirTargetPath([System.IO.FileInfo]$file) {
    $sourceDir = [System.IO.Path]::GetFullPath($file.DirectoryName)
    $defaultDir = [System.IO.Path]::GetFullPath($bundleRoot)
    if ($sourceDir -eq $defaultDir) { return $file.FullName }
    if ($sourceDir.StartsWith($defaultDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        $relative = $sourceDir.Substring($defaultDir.Length + 1)
        return (Join-Path (Join-Path $OutDirAbs $relative) $file.Name)
    }
    return (Join-Path $OutDirAbs $file.Name)
}

function Copy-IntoOutDir([System.IO.FileInfo]$file) {
    $targetPath = Get-OutDirTargetPath $file
    if ($targetPath -eq $file.FullName) { return $file }
    $parent = Split-Path -Parent $targetPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    Copy-Item -LiteralPath $file.FullName -Destination $targetPath -Force
    Write-Detail ('已复制到 ' + $targetPath)
    return (Get-Item -LiteralPath $targetPath)
}

# ---------------------------------------------------------------- 参数规范化
$wanted = New-Object 'System.Collections.Generic.List[string]'
foreach ($entry in $Bundles) {
    foreach ($piece in ($entry -split '[,;\s]+')) {
        $name = $piece.Trim().ToLowerInvariant()
        if ($name -eq '') { continue }
        if ($name -ne 'msi' -and $name -ne 'portable') {
            throw ('未知的 -Bundles 取值：{0}（可选：msi、portable）' -f $piece)
        }
        if (-not $wanted.Contains($name)) { $wanted.Add($name) }
    }
}
if ($wanted.Count -eq 0) { throw '请至少指定一种产物类型：-Bundles msi,portable' }
$wantMsi = $wanted.Contains('msi')
$wantPortable = $wanted.Contains('portable')
$bundleNames = ($wanted -join ', ')

if ($Publish -and -not $wantMsi) {
    throw '生成 latest.json 需要 MSI：请使用 -Bundles msi,portable（默认）把 msi 包含进来。'
}

$OutDirAbs = Resolve-RepoPath $OutDir
if (-not (Test-Path -LiteralPath $OutDirAbs)) {
    New-Item -ItemType Directory -Force -Path $OutDirAbs | Out-Null
}

# ---------------------------------------------------------------- 前置检查
Write-Host ''
Write-Host '============================================================' -ForegroundColor Green
Write-Host '  LanPipe 打包工具：MSI 安装包 + 便携版 zip' -ForegroundColor Green
Write-Host ('  仓库：' + $root) -ForegroundColor Green
Write-Host ('  产物类型：' + $bundleNames) -ForegroundColor Green
Write-Host ('  输出目录：' + $OutDirAbs) -ForegroundColor Green
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

if ($Version -and $Version.Trim() -ne '') {
    $requestedVersion = $Version.Trim()
    Write-Detail ('指定版本：' + $requestedVersion)
    if ($requestedVersion -ne $realVersion) {
        Write-Notice ('指定版本与配置不一致：tauri.conf.json 为 {0}，实际产物按 {0} 命名；如需改版本请同时修改 tauri.conf.json 与 Cargo.toml。' -f $realVersion)
    }
}

$nodeExe = Get-ToolPath 'node' '请安装 Node.js 18+（https://nodejs.org/）并确认 node 在 PATH 中。' '' ''
$npmExe = Get-ToolPath 'npm' 'npm 通常随 Node.js 一起安装，请检查 Node.js 安装是否完整。' '' ''
$npxExe = Get-ToolPath 'npx' 'npx 通常随 Node.js 一起安装，请检查 Node.js 安装是否完整。' '' ''
$cargoDir = Join-Path $env:USERPROFILE '.cargo\bin'
$cargoExe = Get-ToolPath 'cargo' ('请安装 Rust（https://win.rustup.rs）或把 ' + $cargoDir + ' 加入 PATH 后重试。') $cargoDir 'cargo.exe'

$nodeVersion = (& $nodeExe --version 2>&1 | Out-String).Trim()
$cargoVersion = (& $cargoExe --version 2>&1 | Out-String).Trim()
Write-Detail ('node ' + $nodeVersion + '，' + $cargoVersion)

if ($wantMsi) {
    $rustcExe = Get-ToolPath 'rustc' ('请安装 Rust（https://win.rustup.rs）或把 ' + $cargoDir + ' 加入 PATH 后重试。') $cargoDir 'rustc.exe'
    $rustcVersion = (& $rustcExe --version 2>&1 | Out-String).Trim()
    Write-Detail ('rustc ' + $rustcVersion)

    $tauriCmd = Join-Path $root 'node_modules\.bin\tauri.cmd'
    if (-not (Test-Path -LiteralPath $tauriCmd)) {
        throw '未找到 node_modules\.bin\tauri.cmd，请先在仓库根目录执行 npm install（需要 @tauri-apps/cli）。'
    }
    Write-Detail ('tauri CLI：' + $tauriCmd)
}

if ($wantPortable) {
    $makePortable = Join-Path $PSScriptRoot 'make-portable.ps1'
    if (-not (Test-Path -LiteralPath $makePortable)) {
        throw ('未找到便携版脚本：' + $makePortable)
    }
}

if ($Publish) {
    $publishScript = Join-Path $PSScriptRoot 'publish-release.ps1'
    if (-not (Test-Path -LiteralPath $publishScript)) {
        throw ('未找到发布脚本：' + $publishScript)
    }

    # 发布参数提前校验：把下载地址与签名口令的检查放在构建之前，
    # 避免用户等了几分钟 MSI 构建后才发现参数缺失。
    $publishUrl = $Url
    if (-not $publishUrl) { $publishUrl = $env:LANPIPE_RELEASE_URL }
    $publishUrl = ([string]$publishUrl).Trim()
    if (-not $publishUrl) {
        throw '缺少 MSI 下载地址：请加 -Url <地址>，或先设置环境变量 LANPIPE_RELEASE_URL 再重试。'
    }
    if ($publishUrl -notmatch '^https?://') {
        throw ('MSI 下载地址必须以 http:// 或 https:// 开头：' + $publishUrl)
    }
    Write-Detail ('发布地址：' + $publishUrl)
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        throw '缺少更新签名口令：请先设置环境变量 TAURI_SIGNING_PRIVATE_KEY_PASSWORD（本机开发密钥口令为 lanpipe-dev-key；正式发布请使用正式密钥）。'
    }
}
Write-Done '前置检查通过'

# ---------------------------------------------------------------- 前端构建
if ($SkipFrontend) {
    Write-Title '跳过前端构建（-SkipFrontend）'
    $distIndex = Join-Path $root 'dist\index.html'
    if ($wantMsi -and -not (Test-Path -LiteralPath $distIndex)) {
        throw ('跳过前端构建，但 dist\index.html 不存在：tauri build 需要前端产物，请去掉 -SkipFrontend 或先执行 npm run build。')
    }
} else {
    Write-Title '构建前端（npm run build）'
    Write-Detail '执行 tsc --noEmit && vite build ...'
    Push-Location $root
    try {
        & $npmExe run build
        Assert-ExitCode 'npm run build（前端构建）'
    } finally {
        Pop-Location
    }
    $distIndex = Join-Path $root 'dist\index.html'
    if (-not (Test-Path -LiteralPath $distIndex)) {
        throw '前端构建后仍未找到 dist\index.html，请检查 vite.config.ts 与构建日志。'
    }
    Write-Done '前端构建完成（dist\）'
}

# ---------------------------------------------------------------- MSI
$msiFile = $null
if ($wantMsi) {
    Write-Title '构建 MSI 安装包（tauri build --bundles msi）'
    Write-Detail '首次打包会自动下载 WiX 到 %LOCALAPPDATA%\tauri，可能需要几分钟 ...'
    $srcTauri = Join-Path $root 'apps\desktop\src-tauri'
    Push-Location $srcTauri
    try {
        & $npxExe --no-install tauri build --bundles msi
        Assert-ExitCode 'npx --no-install tauri build --bundles msi'
    } finally {
        Pop-Location
    }

    $msiDirs = @(
        (Join-Path $root 'target\release\bundle\msi'),
        (Join-Path $root 'apps\desktop\src-tauri\target\release\bundle\msi')
    )
    $candidates = @()
    foreach ($dir in $msiDirs) {
        if (Test-Path -LiteralPath $dir) {
            $candidates += @(Get-ChildItem -LiteralPath $dir -Filter '*.msi' -File -ErrorAction SilentlyContinue)
        }
    }
    if ($candidates.Count -eq 0) {
        throw 'tauri build 已结束，但未找到 MSI 产物（target\release\bundle\msi\*.msi），请查看上方 tauri 日志。'
    }
    $msiFile = $candidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($msiFile.Name -notlike ('*' + $realVersion + '*')) {
        Write-Notice ('MSI 文件名与配置版本 {0} 不一致：{1}' -f $realVersion, $msiFile.Name)
    }
    Write-Detail ('MSI：' + $msiFile.FullName)
    Write-Done 'MSI 构建完成'
}

# ---------------------------------------------------------------- 便携版
$zipFile = $null
if ($wantPortable) {
    Write-Title '构建便携版 zip（tools\make-portable.ps1）'
    $exePath = Join-Path $root 'target\release\lanpipe.exe'
    if (-not (Test-Path -LiteralPath $exePath)) {
        if ($wantMsi) {
            throw ('MSI 构建后仍未找到 {0}，请检查 tauri build 是否真正完成。' -f $exePath)
        }
        Write-Detail '未找到 target\release\lanpipe.exe，先编译 release 版 ...'
        Push-Location $root
        try {
            & $cargoExe build --release --features custom-protocol -p lanpipe
            Assert-ExitCode 'cargo build --release --features custom-protocol -p lanpipe'
        } finally {
            Pop-Location
        }
    }
    if (-not (Test-Path -LiteralPath $exePath)) {
        throw ('仍未找到 {0}，无法生成便携版。' -f $exePath)
    }

    Push-Location $root
    try {
        $null = & (Join-Path $PSScriptRoot 'make-portable.ps1')
    } finally {
        Pop-Location
    }

    $zipDir = Join-Path $root 'target\release\bundle\portable'
    $expectedZip = Join-Path $zipDir ('LanPipe_{0}_x64-portable.zip' -f $realVersion)
    if (Test-Path -LiteralPath $expectedZip) {
        $zipFile = Get-Item -LiteralPath $expectedZip
    } else {
        $zipFile = Get-ChildItem -LiteralPath $zipDir -Filter '*.zip' -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    if (-not $zipFile) {
        throw ('未找到便携版 zip（{0}），请检查 make-portable.ps1 的输出。' -f $zipDir)
    }
    Write-Detail ('zip：' + $zipFile.FullName)
    Write-Done '便携版构建完成'
}

# ---------------------------------------------------------------- 汇总产物
$artifacts = New-Object 'System.Collections.Generic.List[object]'
if ($msiFile) {
    $msiFile = Copy-IntoOutDir $msiFile
    $artifacts.Add([pscustomobject]@{ Kind = 'MSI'; Path = $msiFile.FullName })
}
if ($zipFile) {
    $zipFile = Copy-IntoOutDir $zipFile
    $artifacts.Add([pscustomobject]@{ Kind = '便携版 zip'; Path = $zipFile.FullName })
}

# ---------------------------------------------------------------- latest.json
if ($Publish) {
    Write-Title '生成自动更新清单 latest.json'
    if (-not $msiFile) {
        throw '生成 latest.json 需要 MSI 产物，但本次没有找到 MSI。'
    }
    # 下载地址与签名口令已在前置检查阶段校验并写入 $publishUrl。
    if (-not $publishUrl) {
        throw '缺少 MSI 下载地址：请加 -Url <地址>，或先设置环境变量 LANPIPE_RELEASE_URL 再重试。'
    }

    $latestJson = Join-Path $OutDirAbs 'latest.json'
    $publishArgs = @{
        Installer = $msiFile.FullName
        Version   = $realVersion
        Url       = $publishUrl
        OutFile   = $latestJson
    }
    if ($Notes -and $Notes.Trim() -ne '') { $publishArgs['Notes'] = $Notes }

    Push-Location $root
    try {
        & (Join-Path $PSScriptRoot 'publish-release.ps1') @publishArgs
        Assert-ExitCode 'publish-release.ps1'
    } finally {
        Pop-Location
    }
    if (-not (Test-Path -LiteralPath $latestJson)) {
        throw ('未生成 latest.json：' + $latestJson)
    }
    $artifacts.Add([pscustomobject]@{ Kind = '更新清单'; Path = $latestJson })
    Write-Done 'latest.json 生成完成'
}

# ---------------------------------------------------------------- 产物汇总
Write-Host ''
Write-Host '================ 产物汇总 ================' -ForegroundColor Green
foreach ($artifact in $artifacts) {
    $file = Get-Item -LiteralPath $artifact.Path
    $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    Write-Host ('[{0}] {1}' -f $artifact.Kind, $file.FullName) -ForegroundColor White
    Write-Host ('       大小：{0:N2} MB（{1:N0} 字节）' -f ($file.Length / 1MB), $file.Length)
    Write-Host ('       SHA256：{0}' -f $hash) -ForegroundColor Yellow
}
Write-Host ''

$elapsed = [int]$stopwatch.Elapsed.TotalSeconds
Write-Done ('打包完成，总耗时 {0} 秒。' -f $elapsed)
exit 0
