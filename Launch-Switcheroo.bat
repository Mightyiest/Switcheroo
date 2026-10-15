@echo off
title Switcheroo - Dota 2 and Steam Switcher
cd /d "%~dp0"
start "" "%~dp0node_modules\electron\dist\electron.exe" "%~dp0."
exit
