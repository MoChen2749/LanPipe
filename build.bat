@echo off
chcp 65001 >nul
setlocal EnableExtensions
title LanPipe Build Tool
pushd "%~dp0"

set "ROOT=%CD%"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
set "RUSTUP=%USERPROFILE%\.cargo\bin\rustup.exe"
set "INSTALL_DIR=%LOCALAPPDATA%\Programs\LanPipe"
if not exist "%CARGO%" set "CARGO=cargo"
if not exist "%RUSTUP%" set "RUSTUP=rustup"
set "PS_EXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%PS_EXE%" set "PS_EXE=powershell"

if /i "%~1"=="build" goto build
if /i "%~1"=="clean" goto clean
if /i "%~1"=="run" goto run
if /i "%~1"=="dev" goto dev
if /i "%~1"=="check" goto check
if /i "%~1"=="cross" goto cross
if /i "%~1"=="package" goto package
if /i "%~1"=="test" goto test


:menu
if defined LP_NO_PAUSE exit /b 0
cls
echo ============================================================
echo   LanPipe Build Tool
echo   Project: %ROOT%
echo ============================================================
echo.
echo   1. Clean build artifacts (dist / dist-bin / target)
echo   2. Build Release single-file exe  [recommended]
echo   3. Run Release build (auto-deploy to LOCALAPPDATA\Programs\LanPipe)
echo   4. Run in dev mode (Vite + cargo run)
echo   5. Cross-compile (Windows ARM64 / Linux WSL / macOS notes)
echo   6. Environment check
echo   7. 打包：MSI 安装包 + 便携版 zip（package）
echo   8. 测试：cargo test + clippy（test）
echo   0. Exit
echo.
echo   用法：build.bat package [publish]   build.bat test
echo.
set "pick="
set /p "pick=Select a number and press Enter: "
if not defined pick goto menu
if "%pick%"=="1" goto clean
if "%pick%"=="2" goto build
if "%pick%"=="3" goto run
if "%pick%"=="4" goto dev
if "%pick%"=="5" goto cross
if "%pick%"=="6" goto check
if "%pick%"=="7" ( set "MENU_CALL=1" & goto package )
if "%pick%"=="8" ( set "MENU_CALL=1" & goto test )
if "%pick%"=="0" goto end
goto menu

:clean
cls
echo [clean] removing dist, dist-bin, target ...
if exist "%ROOT%\dist" rmdir /s /q "%ROOT%\dist"
if exist "%ROOT%\dist-bin" rmdir /s /q "%ROOT%\dist-bin"
if exist "%ROOT%\target" rmdir /s /q "%ROOT%\target"
echo [clean] done (node_modules kept to avoid re-download).
echo.
call :pause_if
goto menu

:build
cls
echo ===== Build Release =====
call :check_env
if errorlevel 1 goto menu
if not exist "%ROOT%\node_modules" (
  echo [1/3] installing frontend dependencies ...
  call npm install
  if errorlevel 1 goto fail
) else (
  echo [1/3] frontend dependencies ready.
)
echo [2/3] building frontend with Vite ...
call npm run build
if errorlevel 1 goto fail
echo [3/3] compiling Rust release with custom-protocol ...
"%CARGO%" build --release --features custom-protocol -p lanpipe
if errorlevel 1 goto fail
call :collect
echo.
echo ===== Build OK =====
echo   workspace copy: %ROOT%\dist-bin\LanPipe.exe
echo   deployed copy:  %INSTALL_DIR%\LanPipe.exe
echo   config file:    %%APPDATA%%\LanPipe\config.json
echo.
echo NOTE: on this dev machine, run the DEPLOYED copy (menu 3).
echo       An exe under the workspace dir is limited by the sandbox.
call :pause_if
goto menu

:run
cls
if not exist "%ROOT%\dist-bin\LanPipe.exe" (
  echo No build output yet. Run option 2 first.
call :pause_if
  goto menu
)
call :collect
echo Starting: %INSTALL_DIR%\LanPipe.exe
start "" "%INSTALL_DIR%\LanPipe.exe"
goto menu

:dev
cls
echo ===== Dev mode =====
call :check_env
if errorlevel 1 goto menu
if not exist "%ROOT%\node_modules" (
  echo installing frontend dependencies ...
  call npm install
  if errorlevel 1 goto fail
)
echo starting Vite dev server in a new window ...
start "LanPipe Vite" cmd /c "npm run dev"
timeout /t 3 /nobreak >nul
echo starting Tauri app (close the app window to return) ...
"%CARGO%" run -p lanpipe
echo.
call :pause_if
goto menu

:cross
cls
echo ===== Cross-compile =====
echo   1. Windows ARM64
echo   2. Linux x64 (needs WSL with Rust + webkit2gtk dev packages)
echo   3. macOS (cannot build from Windows, notes)
echo   0. Back
echo.
set "pick2="
set /p "pick2=Select a number and press Enter: "
if "%pick2%"=="1" goto cross_arm64
if "%pick2%"=="2" goto cross_linux
if "%pick2%"=="3" goto cross_mac
goto menu

:cross_arm64
cls
echo [ARM64] adding rust target ...
"%RUSTUP%" target add aarch64-pc-windows-msvc
echo [ARM64] requires Visual Studio Build Tools ARM64 components; missing ones fail at link time.
"%CARGO%" build --release --features custom-protocol --target aarch64-pc-windows-msvc -p lanpipe
if errorlevel 1 goto fail
echo output: %ROOT%\target\aarch64-pc-windows-msvc\release\lanpipe.exe
call :pause_if
goto menu

:cross_linux
cls
where wsl >nul 2>nul
if errorlevel 1 (
  echo WSL not found. Install it first: wsl --install
call :pause_if
  goto menu
)
echo [Linux] building through WSL (Rust + build-essential + libwebkit2gtk-4.1-dev required inside WSL) ...
for /f "delims=" %%P in ('wsl wslpath -a "%ROOT%"') do set "WSLPATH=%%P"
wsl -e bash -lc "cd '%WSLPATH%' && cargo build --release --features custom-protocol -p lanpipe"
if errorlevel 1 goto fail
echo output: %WSLPATH%/target/release/lanpipe
call :pause_if
goto menu

:cross_mac
cls
echo [macOS] cross-compiling from Windows is not supported:
echo   - needs macOS + Xcode Command Line Tools + Apple SDK;
echo   - build on a Mac: npm run build, then cargo build --release --features custom-protocol;
echo   - or use a GitHub Actions macos-latest runner to produce .app/.dmg.
call :pause_if
goto menu

:check
cls
echo ===== Environment check =====
echo [Node.js]
call node --version
call npm --version
echo.
echo [Rust]
"%CARGO%" --version
"%RUSTUP%" --version
"%RUSTUP%" show
echo.
echo [MSVC Build Tools]
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
  for /f "delims=" %%I in ('"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath') do echo   %%I
) else (
  echo   vswhere / VS Build Tools not detected
)
echo.
echo [WebView2 runtime]
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv 2>nul | findstr "pv"
echo.
call :pause_if
goto menu

rem ============================================================
rem  package：一条命令产出 MSI + 便携版 zip（调用 tools\package.ps1）
rem  package publish：额外生成 latest.json（需要 LANPIPE_RELEASE_URL）
rem ============================================================
:package
if /i "%~2"=="publish" goto package_publish
cls
echo ===== 打包：MSI 安装包 + 便携版 zip =====
echo.
if not exist "%ROOT%\tools\package.ps1" (
  echo [错误] 缺少 tools\package.ps1，无法打包。
  set "PKG_RC=1"
  goto package_done
)
echo 调用 tools\package.ps1（MSI 由 Tauri 构建，便携版由 tools\make-portable.ps1 构建）
echo.
"%PS_EXE%" -NoProfile -ExecutionPolicy Bypass -File "%ROOT%\tools\package.ps1" -Bundles msi,portable
set "PKG_RC=%ERRORLEVEL%"
goto package_done

:package_publish
cls
echo ===== 打包并发布：MSI + 便携版 zip + latest.json =====
echo.
if not exist "%ROOT%\tools\package.ps1" (
  echo [错误] 缺少 tools\package.ps1，无法打包。
  set "PKG_RC=1"
  goto package_done
)
if not exist "%ROOT%\tools\publish-release.ps1" (
  echo [错误] 缺少 tools\publish-release.ps1，无法生成 latest.json。
  set "PKG_RC=1"
  goto package_done
)
if not defined LANPIPE_RELEASE_URL (
  echo [错误] 未设置环境变量 LANPIPE_RELEASE_URL。
  echo        它应指向 MSI 的公网下载地址，例如：
  echo          set "LANPIPE_RELEASE_URL=https://example.com/lanpipe/LanPipe_0.1.0_x64_zh-CN.msi"
  echo        设置后重新运行：build.bat package publish
  set "PKG_RC=1"
  goto package_done
)
echo 下载地址取自环境变量 LANPIPE_RELEASE_URL（此处不打印，避免其中的特殊字符破坏输出）
echo.
"%PS_EXE%" -NoProfile -ExecutionPolicy Bypass -File "%ROOT%\tools\package.ps1" -Bundles msi,portable -Publish -Url "%LANPIPE_RELEASE_URL%"
set "PKG_RC=%ERRORLEVEL%"
goto package_done

:package_done
echo.
if not "%PKG_RC%"=="0" (
  echo [失败] 打包未完成，退出码 %PKG_RC%，详见上方日志。
) else (
  echo [完成] 打包成功，产物见上方汇总（路径 / 大小 / SHA256）。
)
if defined MENU_CALL (
  set "MENU_CALL="
  call :pause_if
  goto menu
)
popd
endlocal & exit /b %PKG_RC%

rem ============================================================
rem  test：cargo test + clippy -D warnings（代码门禁）
rem ============================================================
:test
cls
echo ===== 测试：cargo test + clippy =====
echo.
"%CARGO%" --version >nul 2>nul
if errorlevel 1 (
  echo [错误] 未找到 cargo。请先安装 Rust 工具链：https://win.rustup.rs
  echo        或把 %USERPROFILE%\.cargo\bin 加入 PATH 后重试。
  set "TEST_RC=1"
  goto test_done
)
set "CARGO_PROFILE_DEV_DEBUG=0"
echo [1/2] cargo test --workspace
echo.
"%CARGO%" test --workspace
if errorlevel 1 (
  set "TEST_RC=1"
  goto test_done
)
echo.
echo [2/2] cargo clippy --workspace -- -D warnings
echo.
"%CARGO%" clippy --workspace --all-targets -- -D warnings
if errorlevel 1 (
  set "TEST_RC=1"
  goto test_done
)
set "TEST_RC=0"

:test_done
echo.
if not "%TEST_RC%"=="0" (
  echo [失败] cargo test 或 clippy 未通过，退出码 %TEST_RC%，详见上方日志。
) else (
  echo [完成] cargo test 与 clippy 全部通过。
)
if defined MENU_CALL (
  set "MENU_CALL="
  call :pause_if
  goto menu
)
popd
endlocal & exit /b %TEST_RC%

:collect
taskkill /IM LanPipe.exe /F >nul 2>nul
timeout /t 1 /nobreak >nul
if exist "%ROOT%\dist-bin" rmdir /s /q "%ROOT%\dist-bin"
mkdir "%ROOT%\dist-bin"
copy /y "%ROOT%\target\release\lanpipe.exe" "%ROOT%\dist-bin\LanPipe.exe" >nul
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
copy /y "%ROOT%\target\release\lanpipe.exe" "%INSTALL_DIR%\LanPipe.exe" >nul
for %%F in ("%INSTALL_DIR%\LanPipe.exe") do echo   binary size: %%~zF bytes
exit /b 0

:check_env
where node >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Node.js not found, install from https://nodejs.org/
  exit /b 1
)
"%CARGO%" --version >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Rust/Cargo not found, install from https://win.rustup.rs/x86_64
  exit /b 1
)
exit /b 0

:fail
echo.
echo [FAILED] build interrupted, see errors above.
call :pause_if
goto menu

:pause_if
if defined LP_NO_PAUSE exit /b 0
pause
exit /b 0

:end
popd
endlocal
exit /b 0
