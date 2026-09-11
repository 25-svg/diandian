@echo off
setlocal
if not "%~1"=="" exit /b 2
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Set-PublisherCredential.ps1" -Publish
exit /b %errorlevel%
