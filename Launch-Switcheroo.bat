@echo off
title Switcheroo - Dota 2 and Steam Switcher
cd /d "%~dp0"
if not exist "%~dp0target\release\Switcheroo.exe" (
  cargo build --release
  if errorlevel 1 exit /b 1
)
start "" "%~dp0target\release\Switcheroo.exe"
exit
