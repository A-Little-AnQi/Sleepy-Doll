@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo Node.js 22.13 or newer is required. Install it and run this file again.
  pause
  exit /b 1
)
node "video\make-video.mjs" %*
set "VIDEO_EXIT=%ERRORLEVEL%"
if not "%VIDEO_EXIT%"=="0" echo Video creation failed. Read the error above.
if "%VIDEO_EXIT%"=="0" if "%~1"=="" start "" "%~dp0video\output\sleepy-doll.mp4"
if "%~1"=="" pause
exit /b %VIDEO_EXIT%
