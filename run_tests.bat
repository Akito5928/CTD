@echo off
setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"
if errorlevel 1 goto :error

echo.
echo Running tests ...
"%VPY%" -m pytest
if errorlevel 1 goto :error

echo.
echo All tests passed.
goto :end

:error
echo.
echo [ERROR] Failed. Please read the messages above.

:end
echo.
pause
endlocal
