@echo off
rem Development helper: full rebuild-and-inject cycle against the test instance.
rem
rem Must run from one elevated shell: a loaded DLL locks its file, BetterGI runs
rem elevated, and the injector needs elevation too.

set "TESTDIR=E:\tools\test\BetterGI"

echo [1/5] Stopping anything holding the bridge DLLs...
taskkill /F /IM BetterGI.exe >nul 2>&1
taskkill /F /IM SleepyTarget.exe >nul 2>&1
timeout /t 3 /nobreak >nul

echo [2/5] Building...
cd /d "%~dp0.."
call build.cmd
if errorlevel 1 (
  echo BUILD FAILED
  exit /b 1
)

echo [3/5] Starting BetterGI...
start "" "%TESTDIR%\BetterGI.exe"
timeout /t 15 /nobreak >nul

echo [4/5] Injecting...
cd /d "%~dp0..\..\target\bridge"
rem Logs append, so clear them first.
del /q user\log\*.log >nul 2>&1
BgiBridge.Injector.exe --process BetterGI.exe
timeout /t 8 /nobreak >nul

echo [5/5] Done. Logs:
echo   %~dp0..\..\target\bridge\user\log\bootstrap.log
echo   %~dp0..\..\target\bridge\user\log\bridge.log
echo   %~dp0..\..\target\bridge\user\log\injector.log
echo.
echo Bridge should now answer on the address in bridge.config.json.
exit /b 0
