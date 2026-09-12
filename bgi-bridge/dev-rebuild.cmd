@echo off
rem Development helper: stop anything holding the bridge DLLs, then rebuild.
rem
rem Needed because a loaded DLL locks its file, and build.cmd cannot clear dist/
rem while that is the case. BetterGI runs elevated, so this has to run elevated
rem too - hence the runas dance in the calling shell.
rem
rem This is a dev convenience only; shipping users never need it.

echo Stopping processes that may hold the bridge DLLs...
taskkill /F /IM BetterGI.exe >nul 2>&1
taskkill /F /IM SleepyTarget.exe >nul 2>&1
timeout /t 3 /nobreak >nul

cd /d "%~dp0"
call build.cmd
exit /b %errorlevel%
