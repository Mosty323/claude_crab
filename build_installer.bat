@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build_installer.ps1"
if errorlevel 1 (
  echo.
  echo Build fehlgeschlagen.
) else (
  explorer "%~dp0dist"
)
pause
