@echo off
setlocal
cd /d "%~dp0"
if exist "deadlock-mod-builder.exe" (
  start "" "%~dp0deadlock-mod-builder.exe"
  exit /b 0
)
if exist "src-tauri\target\x86_64-pc-windows-msvc\release\deadlock-mod-builder.exe" (
  start "" "%~dp0src-tauri\target\x86_64-pc-windows-msvc\release\deadlock-mod-builder.exe"
  exit /b 0
)
if exist "src-tauri\target\release\deadlock-mod-builder.exe" (
  start "" "%~dp0src-tauri\target\release\deadlock-mod-builder.exe"
  exit /b 0
)
echo The Rust application has not been built in this source folder.
echo Install the supplied Rust Windows installer, or run Build-Rust-Windows.cmd.
echo Launch.cmd also starts the current Rust application.
pause
exit /b 1
