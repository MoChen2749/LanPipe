# 生成自动更新清单 latest.json（配合 tauri-plugin-updater）
# 用法示例：
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "lanpipe-dev-key"
#   .\tools\publish-release.ps1 -Installer "target\release\bundle\msi\LanPipe_0.1.1_x64_zh-CN.msi" `
#       -Version "0.1.1" -Notes "修复若干问题" -Url "https://example.com/lanpipe/LanPipe_0.1.1_x64_zh-CN.msi"
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Url,
    [string]$Notes = "",
    [string]$KeyPath = ".keys/lanpipe-updater.key",
    [string]$OutFile = "target/release/bundle/latest.json"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath([string]$path) {
    if ([System.IO.Path]::IsPathRooted($path)) { return $path }
    return (Join-Path $root $path)
}

$installerPath = Resolve-RepoPath $Installer
$keyFile = Resolve-RepoPath $KeyPath
$outPath = Resolve-RepoPath $OutFile
if (-not (Test-Path -LiteralPath $installerPath)) { throw "找不到安装包：$installerPath" }
if (-not (Test-Path -LiteralPath $keyFile)) { throw "找不到私钥：$keyFile" }
if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
    throw "请先设置 TAURI_SIGNING_PRIVATE_KEY_PASSWORD（本次开发密钥口令为 lanpipe-dev-key，正式发布请换密钥）"
}

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (-not ($env:PATH -split ";" | Where-Object { $_ -ieq $cargoBin })) {
    $env:PATH = "$cargoBin;$env:PATH"
}

Push-Location $root
try {
    npx --no-install tauri signer sign --private-key-path $keyFile --password $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD --app-version $Version $installerPath
    if ($LASTEXITCODE -ne 0) { throw "tauri signer sign 失败（exit=$LASTEXITCODE）" }
} finally {
    Pop-Location
}

$sigPath = "$installerPath.sig"
if (-not (Test-Path -LiteralPath $sigPath)) { throw "未生成签名文件：$sigPath" }
$signature = (Get-Content -LiteralPath $sigPath -Raw).Trim()

$manifest = [ordered]@{
    version   = $Version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = $signature
            url       = $Url
        }
    }
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outPath) | Out-Null
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $outPath -Encoding UTF8
Write-Host "latest.json -> $outPath"
Write-Host ($manifest | ConvertTo-Json -Depth 6)
