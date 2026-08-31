@echo off
setlocal
set "ROOT=%~dp0.."
where skilldo >nul 2>nul
if %errorlevel%==0 (skilldo device pull %*) else if exist "%ROOT%\src-tauri\target\release\skilldo.exe" ("%ROOT%\src-tauri\target\release\skilldo.exe" device pull %*) else if exist "%ROOT%\src-tauri\target\debug\skilldo.exe" ("%ROOT%\src-tauri\target\debug\skilldo.exe" device pull %*) else (echo SkillDo CLI not found. Run scripts\build.sh cli first. & exit /b 1)
