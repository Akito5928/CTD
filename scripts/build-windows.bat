@echo off
rem CTD v1.1 - Windows build script (ASCII only, CRLF).
rem Everything is installed INSIDE this project folder (tools\) - nothing is
rem installed system-wide and PATH/registry are not modified.
rem Delete the whole folder to remove everything.
rem
rem Usage:  scripts\build-windows.bat [build|test|bench]     (default: build)

setlocal EnableExtensions
cd /d "%~dp0.."
set "ROOT=%CD%"
set "TOOLS=%ROOT%\tools"
set "MODE=%~1"
if "%MODE%"=="" set "MODE=build"
set "VERSION=1.1.0"

set "RUSTUP_HOME=%TOOLS%\rustup"
set "CARGO_HOME=%TOOLS%\cargo"
set "CARGO_TARGET_DIR=%ROOT%\target"
set "W64DIR=%TOOLS%\w64devkit"
set "PATH=%CARGO_HOME%\bin;%W64DIR%\bin;%PATH%"
set "RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu"
set "CC_x86_64_pc_windows_gnu=gcc"
set "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=gcc"

if not exist "%TOOLS%" mkdir "%TOOLS%"

rem ---- 1. C compiler (portable MinGW-w64, ~65 MB download) ----
if exist "%W64DIR%\bin\gcc.exe" goto have_gcc
echo [1/3] Downloading portable MinGW-w64 (w64devkit) into tools\ ...
curl.exe -L --fail -o "%TOOLS%\w64devkit.7z.exe" "https://github.com/skeeto/w64devkit/releases/download/v2.11.0/w64devkit-x64-2.11.0.7z.exe"
if errorlevel 1 goto fail
"%TOOLS%\w64devkit.7z.exe" -y -o"%TOOLS%"
if errorlevel 1 goto fail
del "%TOOLS%\w64devkit.7z.exe"
if not exist "%W64DIR%\bin\gcc.exe" (
  echo ERROR: gcc.exe not found after extraction. Expected %W64DIR%\bin\gcc.exe
  goto fail
)
:have_gcc

rem ---- 2. Rust toolchain (project-local rustup, GNU host) ----
if exist "%CARGO_HOME%\bin\cargo.exe" goto have_rust
echo [2/3] Installing Rust (project-local) into tools\ ...
curl.exe -L --fail -o "%TOOLS%\rustup-init.exe" "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
if errorlevel 1 goto fail
"%TOOLS%\rustup-init.exe" -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain stable
if errorlevel 1 goto fail
del "%TOOLS%\rustup-init.exe"
:have_rust

echo [3/3] Running mode: %MODE%
cargo --version
rustc --version
gcc --version | findstr /n "gcc"

if /i "%MODE%"=="test" goto do_test
if /i "%MODE%"=="bench" goto do_bench
if /i "%MODE%"=="build" goto do_build
echo usage: scripts\build-windows.bat [build^|test^|bench]
exit /b 2

:do_test
cargo test --release --locked
if errorlevel 1 goto fail
echo.
echo TESTS PASSED
exit /b 0

:do_bench
cargo build --release --locked
if errorlevel 1 goto fail
cargo run --release --locked --example bench -- --sizes 1000,10000,100000 > bench\results-windows.md
if errorlevel 1 goto fail
echo Benchmark written to bench\results-windows.md
exit /b 0

:do_build
cargo test --release --locked
if errorlevel 1 goto fail
cargo build --release --locked
if errorlevel 1 goto fail
if not exist "%ROOT%\dist" mkdir "%ROOT%\dist"
copy /y "%CARGO_TARGET_DIR%\release\ctd.exe" "%ROOT%\dist\ctd-v%VERSION%-windows-x86_64.exe" >nul
if errorlevel 1 goto fail
echo.
echo BUILD OK: dist\ctd-v%VERSION%-windows-x86_64.exe
"%ROOT%\dist\ctd-v%VERSION%-windows-x86_64.exe" version
exit /b 0

:fail
echo.
echo FAILED. Copy the output above and send it back.
exit /b 1
