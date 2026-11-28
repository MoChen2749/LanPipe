# LanPipe Linux 构建入口（Windows）。
#
# 用法:
#   powershell -File tools\build-linux.ps1 -Command cli
#   powershell -File tools\build-linux.ps1 -Command desktop -Distro lanpipe-build
#   powershell -File tools\build-linux.ps1 -Command bundle
#   powershell -File tools\build-linux.ps1 -Command deps
#
# cli     在 Windows 上交叉编译 Linux 静态 CLI（x86_64-unknown-linux-musl，无需 WSL）
# desktop 通过 WSL 构建桌面程序
# bundle  通过 WSL 构建 deb + AppImage
# deps    在 WSL 中检查依赖

param(
  [ValidateSet("cli", "desktop", "bundle", "deps")]
  [string]$Command = "cli",
  [string]$Distro = ""
)

$ErrorActionPreference = "Continue"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$Rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
$Rustc = Join-Path $env:USERPROFILE ".cargo\bin\rustc.exe"
if (-not (Test-Path $Cargo)) { $Cargo = "cargo" }
if (-not (Test-Path $Rustup)) { $Rustup = "rustup" }
if (-not (Test-Path $Rustc)) { $Rustc = "rustc" }

function Write-Step([string]$Text) {
  Write-Host ""
  Write-Host "== $Text"
}

function Build-Cli {
  $target = "x86_64-unknown-linux-musl"
  Write-Step "准备 Rust 目标 $target"
  $installed = & $Rustup target list --installed
  if ($installed -notcontains $target) {
    & $Rustup target add $target
    if ($LASTEXITCODE -ne 0) { throw "rustup target add $target 失败" }
  }
  Write-Host "  已安装"

  $sysroot = (& $Rustc --print sysroot).Trim()
  $hostTriple = "x86_64-pc-windows-msvc"
  $lld = Join-Path $sysroot "lib\rustlib\$hostTriple\bin\rust-lld.exe"
  if (-not (Test-Path $lld)) { throw "未找到 rust-lld: $lld" }

  Write-Step "交叉编译 CLI"
  $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = $lld
  $env:RUSTFLAGS = "-C link-self-contained=yes -C linker-flavor=ld.lld"
  & $Cargo build -p lanpipe-cli --release --target $target
  if ($LASTEXITCODE -ne 0) { throw "交叉编译失败" }

  $bin = Join-Path $Root "target\$target\release\lanpipe-cli"
  if (-not (Test-Path $bin)) { throw "产物不存在: $bin" }
  $outDir = Join-Path $Root "dist-linux"
  New-Item -ItemType Directory -Force -Path $outDir | Out-Null
  $out = Join-Path $outDir "lanpipe-cli"
  Copy-Item -LiteralPath $bin -Destination $out -Force

  $bytes = [System.IO.File]::ReadAllBytes($out)
  $magic = "$([char]$bytes[1])$([char]$bytes[2])$([char]$bytes[3])"
  if ($bytes[0] -ne 0x7F -or $magic -ne "ELF") { throw "产物不是 ELF 文件: $out" }

  $hash = (Get-FileHash -LiteralPath $out -Algorithm SHA256).Hash
  Write-Step "产物"
  Write-Host "  $out"
  Write-Host "  size: $($bytes.Length) bytes"
  Write-Host "  sha256: $hash"
  Write-Host "  类型: ELF 静态可执行文件"
}

function Get-WslDistro {
  $names = @(& wsl.exe -l -q 2>$null | ForEach-Object { ($_ -replace "`0", "").Trim() } | Where-Object { $_ })
  if ($Distro) {
    if ($names -notcontains $Distro) { throw "WSL 发行版不存在: $Distro；可用: $($names -join ', ')" }
    return $Distro
  }
  $pick = $names | Where-Object { $_ -match "Ubuntu|lanpipe" } | Select-Object -First 1
  if (-not $pick) { throw "未找到 WSL 发行版；可用: $($names -join ', ')" }
  return $pick
}

function ConvertTo-WslPath([string]$WinPath) {
  $full = [System.IO.Path]::GetFullPath($WinPath)
  $drive = $full.Substring(0, 1).ToLowerInvariant()
  $rest = $full.Substring(2).Replace("\", "/")
  return "/mnt/$drive$rest"
}

function Invoke-InWsl([string]$ScriptCommand) {
  $pick = Get-WslDistro
  $linuxRoot = ConvertTo-WslPath $Root
  # Windows 侧 node_modules 是 win32 版本（rollup/tauri CLI 的原生包不通用），
  # 所以 WSL 内必须在 ext4 的工作副本里构建；这里先同步源码，再构建，最后把产物拷回。
  $wslSrc = '/root/lanpipe-src'
  if ($env:LANPIPE_WSL_SRC) { $wslSrc = $env:LANPIPE_WSL_SRC }
  $linuxOut = ConvertTo-WslPath (Join-Path $Root 'dist-linux')
  $bash = @"
set -e
export PATH="/root/.cargo/bin:`$PATH"
SRC='$linuxRoot'
DST='$wslSrc'
OUT='$linuxOut'
mkdir -p "`$DST" "`$OUT"
rsync -a --delete --exclude target --exclude target-linux --exclude node_modules --exclude dist-bin --exclude .git --exclude artifacts --exclude dist-linux "`$SRC/" "`$DST/"
cd "`$DST"
if [ ! -d node_modules ] || [ package-lock.json -nt node_modules/.package-lock.json ]; then npm ci; fi
$ScriptCommand
if [ -d dist-linux ]; then rsync -a dist-linux/ "`$OUT/"; fi
"@
  # PowerShell → wsl.exe 的多行参数会被拆坏，这里先落成脚本文件再让 bash 执行。
  $shPath = Join-Path $Root 'target\wsl-build.sh'
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $shPath) | Out-Null
  [System.IO.File]::WriteAllText($shPath, ($bash -replace "`r`n", "`n"), (New-Object System.Text.UTF8Encoding($false)))
  $shWsl = ConvertTo-WslPath $shPath
  Write-Step "在 WSL ($pick) 中构建（工作副本 $wslSrc）"
  & wsl.exe -d $pick -u root -- bash $shWsl
  if ($LASTEXITCODE -ne 0) { throw "WSL 构建失败（退出码 $LASTEXITCODE）" }
}

switch ($Command) {
  "cli" { Build-Cli }
  "deps" { Invoke-InWsl "bash tools/build-linux.sh deps" }
  "desktop" { Invoke-InWsl "bash tools/build-linux.sh desktop" }
  "bundle" { Invoke-InWsl "bash tools/build-linux.sh bundle" }
}
exit 0
