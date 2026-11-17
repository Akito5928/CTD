@echo off
rem Common setup used by build_exe.bat and run_tests.bat. Do not run directly.
rem Creates a virtual environment in .venv inside this folder (nothing is installed globally).
rem   Mode A: tools\uv.exe exists -> Python 3.12 is downloaded into .local\ (no installer, no PATH, no registry)
rem   Mode B: otherwise            -> uses Python 3.11+ already installed on this PC
rem Sets VPY to the venv python. Returns errorlevel 1 on failure.
cd /d "%~dp0"
set "VPY=%~dp0.venv\Scripts\python.exe"
set "PIP_CACHE_DIR=%~dp0.local\pip-cache"
set "PIP_DISABLE_PIP_VERSION_CHECK=1"
if exist "%VPY%" goto :deps
if exist "%~dp0tools\uv.exe" goto :uvmode

rem ---- Mode B: system Python ----
set "PY="
py -3 --version >nul 2>&1 && set "PY=py -3"
if not defined PY python --version >nul 2>&1 && set "PY=python"
if not defined PY goto :nopython
%PY% -c "import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)" >nul 2>&1
if errorlevel 1 goto :oldpython
echo Creating .venv with:
%PY% --version
%PY% -m venv "%~dp0.venv"
if errorlevel 1 exit /b 1
goto :deps

rem ---- Mode A: portable Python via uv ----
:uvmode
set "UV_PYTHON_INSTALL_DIR=%~dp0.local\python"
set "UV_CACHE_DIR=%~dp0.local\uv-cache"
set "UV_PYTHON_INSTALL_REGISTRY=0"
set "UV_PYTHON_INSTALL_BIN=0"
echo Using tools\uv.exe. Downloading Python 3.12 into .local\ (first time only, internet required) ...
"%~dp0tools\uv.exe" venv "%~dp0.venv" --python 3.12 --python-preference only-managed --seed
if errorlevel 1 exit /b 1
goto :deps

:deps
echo Installing requirements into .venv ...
"%VPY%" -m pip install -r "%~dp0requirements-dev.txt"
if errorlevel 1 exit /b 1
exit /b 0

:nopython
echo [ERROR] Python was not found.
echo   Option 1 (no install): put uv.exe into the tools folder. See README.md.
echo   Option 2: install Python 3.11 or later from https://www.python.org/
exit /b 1

:oldpython
echo [ERROR] Python 3.11 or later is required. Found:
%PY% --version
echo   Option: put uv.exe into the tools folder (see README.md) to use a portable Python.
exit /b 1
