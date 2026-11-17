@echo off
setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"
if errorlevel 1 goto :error

echo.
echo Building ctd.exe ...
set "PYINSTALLER_CONFIG_DIR=%~dp0.local\pyinstaller"
set "ICONOPT="
if exist "%~dp0assets\ctd.ico" set ICONOPT=--icon "%~dp0assets\ctd.ico"
"%VPY%" -m PyInstaller --onefile --console --clean --noconfirm %ICONOPT% --name ctd --paths src ctd_entry.py
if errorlevel 1 goto :error

echo.
echo Done: %~dp0dist\ctd.exe
echo Copy dist\ctd.exe wherever you like. Deleting this whole folder removes everything else.
goto :end

:error
echo.
echo [ERROR] Build failed. Please read the messages above.

:end
echo.
pause
endlocal
