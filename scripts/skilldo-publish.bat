@echo off
setlocal
set /p "ANSWER=This may commit and push your owned Skill repositories, then update WebDAV. Continue? [y/N] "
if /I not "%ANSWER%"=="y" if /I not "%ANSWER%"=="yes" (echo Cancelled. & exit /b 0)
set "ROOT=%~dp0.."
where skilldo >nul 2>nul
if %errorlevel%==0 (skilldo device publish --yes %*) else if exist "%ROOT%\src-tauri\target\release\skilldo.exe" ("%ROOT%\src-tauri\target\release\skilldo.exe" device publish --yes %*) else if exist "%ROOT%\src-tauri\target\debug\skilldo.exe" ("%ROOT%\src-tauri\target\debug\skilldo.exe" device publish --yes %*) else (echo SkillDo CLI not found. Run scripts\build.sh cli first. & exit /b 1)
