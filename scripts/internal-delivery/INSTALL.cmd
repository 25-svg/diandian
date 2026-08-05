@echo off
setlocal EnableExtensions
set "INSTALL_SCRIPT=%~dp0install.ps1"

if not exist "%INSTALL_SCRIPT%" (
    echo.
    echo Installation files are incomplete.
    echo Please extract the whole ZIP file before running this launcher.
    echo.
    pause
    exit /b 2
)

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%INSTALL_SCRIPT%" %*
set "EXIT_CODE=%ERRORLEVEL%"

if /I "%~1"=="-ValidateOnly" exit /b %EXIT_CODE%
if /I "%~1"=="-TestInstall" exit /b %EXIT_CODE%

echo.
if not "%EXIT_CODE%"=="0" echo Installation failed. Error code: %EXIT_CODE%
pause
exit /b %EXIT_CODE%
