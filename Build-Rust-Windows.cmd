@echo off
setlocal
cd /d "%~dp0"
where pwsh >nul 2>nul
if errorlevel 1 (
  echo PowerShell 7 is required for the build script. See docs\DEVELOPMENT.md.
  pause
  exit /b 1
)
pwsh -NoProfile -File "%~dp0scripts\build-windows.ps1" %*
set "DMB_BUILD_EXIT=%ERRORLEVEL%"
if not "%DMB_BUILD_EXIT%"=="0" echo Build failed. See artifacts\validation for details.
pause
exit /b %DMB_BUILD_EXIT%
