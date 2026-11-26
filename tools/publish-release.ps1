# 生成自动更新清单 latest.json（配合 tauri-plugin-updater）
# 用法示例（本地）：
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "lanpipe-dev-key"
#   .\tools\publish-release.ps1 -Installer "target\release\bundle\msi\LanPipe_0.1.1_x64_zh-CN.msi" `
#       -Version "0.1.1" -Notes "修复若干问题" -Url "https://example.com/lanpipe/LanPipe_0.1.1_x64_zh-CN.msi"
# CI 用法：私钥内容放在环境变量 TAURI_SIGNING_PRIVATE_KEY，口令放在 TAURI_SIGNING_PRIVATE_KEY_PASSWORD
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Url,
    [string]$Notes = "",
    [string]$KeyPath = ".keys/lanpipe-updater.key",
    [string]$Password = "",
    [string]$OutFile = "target/release/bundle/latest.json"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath([string]$path) {
    if ([System.IO.Path]::IsPathRooted($path)) { return $path }
    return (Join-Path $root $path)
}

$installerPath = Resolve-RepoPath $Installer
$outPath = Resolve-RepoPath $OutFile
if (-not (Test-Path -LiteralPath $installerPath)) { throw "找不到安装包：$installerPath" }

$signPassword = if ($Password) { $Password } else { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD }
if (-not $signPassword) {
    throw "请先设置 TAURI_SIGNING_PRIVATE_KEY_PASSWORD（本次开发密钥口令为 lanpipe-dev-key，正式发布请换密钥）"
}

# 私钥优先用文件（本地）；CI 里 .keys/ 不入库，改用 TAURI_SIGNING_PRIVATE_KEY 内容写临时文件。
$keyFile = Resolve-RepoPath $KeyPath
$tempKey = $null
if (-not (Test-Path -LiteralPath $keyFile)) {
    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
        throw "找不到私钥：$keyFile，且未设置环境变量 TAURI_SIGNING_PRIVATE_KEY"
    }
    $tempKey = Join-Path ([System.IO.Path]::GetTempPath()) ("lanpipe-updater-" + [guid]::NewGuid().ToString("N") + ".key")
    [System.IO.File]::WriteAllText($tempKey, $env:TAURI_SIGNING_PRIVATE_KEY, (New-Object System.Text.UTF8Encoding($false)))
    $keyFile = $tempKey
}

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (-not ($env:PATH -split ";" | Where-Object { $_ -ieq $cargoBin })) {
    $env:PATH = "$cargoBin;$env:PATH"
}

# tauri signer 不允许同时用 --private-key-path 和 TAURI_SIGNING_PRIVATE_KEY，临时去掉环境变量避免参数冲突。
$savedKeyEnv = $env:TAURI_SIGNING_PRIVATE_KEY
if ($tempKey) { Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue }

Push-Location $root
try {
    npx --no-install tauri signer sign --private-key-path $keyFile --password $signPassword --app-version $Version $installerPath
    if ($LASTEXITCODE -ne 0) { throw "tauri signer sign 失败（exit=$LASTEXITCODE）" }
} finally {
    Pop-Location
    if ($savedKeyEnv) { $env:TAURI_SIGNING_PRIVATE_KEY = $savedKeyEnv }
    if ($tempKey -and (Test-Path -LiteralPath $tempKey)) { Remove-Item -LiteralPath $tempKey -Force -ErrorAction SilentlyContinue }
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
# JSON 必须是 UTF-8 无 BOM：serde_json 不接受 BOM，否则更新检查会解析失败。
$json = $manifest | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText($outPath, $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "latest.json -> $outPath"
Write-Host $json
