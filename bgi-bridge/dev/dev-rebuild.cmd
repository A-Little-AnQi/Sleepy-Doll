@echo off
rem Development helper: stop anything holding the bridge DLLs, then rebuild.
rem
rem A loaded DLL locks its file and build.cmd cannot clear dist/ while that is
rem the case; BetterGI runs elevated, so this has to run elevated too.

echo Stopping processes that may hold the bridge DLLs...
taskkill /F /IM BetterGI.exe >nul 2>&1
taskkill /F /IM SleepyTarget.exe >nul 2>&1
timeout /t 3 /nobreak >nul

cd /d "%~dp0.."
call build.cmd
exit /b %errorlevel%
