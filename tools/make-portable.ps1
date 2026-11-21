# 生成便携版 zip：exe + LICENSE + 简明说明
# 用法： pwsh -ExecutionPolicy Bypass -File tools/make-portable.ps1
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
$exe = Join-Path $root 'target\release\lanpipe.exe'
if (-not (Test-Path $exe)) { throw "missing $exe; run cargo build --release --features custom-protocol -p lanpipe first" }

$stage = Join-Path $root 'target\portable'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $exe (Join-Path $stage 'LanPipe.exe')
$license = Join-Path $root 'LICENSE'
if (Test-Path $license) { Copy-Item $license (Join-Path $stage 'LICENSE') }

$lines = @(
    "LanPipe $version 便携版",
    "=====================",
    "",
    "双击 LanPipe.exe 直接运行，无需安装；数据保存在 %APPDATA%\LanPipe。",
    "",
    "- 接收的文件默认保存到：%USERPROFILE%\Downloads\LanPipe",
    "- 防火墙放行、开机自启、托盘可在「设置」里配置",
    "- 卸载：先关闭开机自启，再删除本目录即可",
    "",
    "项目主页与文档见仓库 README。"
)
Set-Content -Path (Join-Path $stage '使用说明.txt') -Value $lines -Encoding UTF8

$outDir = Join-Path $root 'target\release\bundle\portable'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$zip = Join-Path $outDir "LanPipe_${version}_x64-portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal
Get-Item $zip | Select-Object FullName, Length
