@echo off
setlocal
rem Keep this file ASCII-only: cmd.exe reads the system OEM code page.
cd /d "%~dp0.."
set "ROOT=%CD%"
set "OUT=%ROOT%\dist\Sleepy-Doll"
set "TEMP=%ROOT%\target\tmp"
set "TMP=%TEMP%"
if not exist "%TEMP%" mkdir "%TEMP%"
if errorlevel 1 exit /b 1

set "CARGO=cargo"
where cargo >nul 2>&1
if errorlevel 1 (
  set "CARGO="
  if exist "%CARGO_HOME%\bin\cargo.exe" set "CARGO=%CARGO_HOME%\bin\cargo.exe"
  if not defined CARGO if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
  if not defined CARGO (
    echo cargo not found. Install Rust or add its bin directory to PATH.
    exit /b 1
  )
)
for /f "tokens=2 delims== " %%V in ('findstr /b /c:"version = " Cargo.toml') do set "VERSION=%%~V"
if not defined VERSION exit /b 1

echo [1/4] Building bridge...
call bgi-bridge\build.cmd
if errorlevel 1 exit /b 1

echo [2/4] Building interface...
call npm run build
if errorlevel 1 exit /b 1

echo [3/4] Building desktop...
"%CARGO%" build --release
if errorlevel 1 exit /b 1

rem Preserve the working installation's configuration and user data.
if not exist "%OUT%\bridge" mkdir "%OUT%\bridge"
copy /y "target\release\sleepy-doll.exe" "%OUT%\" >nul
if errorlevel 1 exit /b 1
for %%F in (
  BgiBridge.Injector.exe
  BgiBridge.Bootstrap.dll
  BgiBridge.dll
  BgiBridge.runtimeconfig.json
  BgiBridge.deps.json
  BgiBridge.Recovery.exe
  BgiBridge.Recovery.dll
  BgiBridge.Recovery.runtimeconfig.json
  BgiBridge.Recovery.deps.json
) do (
  copy /y "target\bridge\%%F" "%OUT%\bridge\" >nul
  if errorlevel 1 (
    echo Could not replace bridge\%%F. Close the process holding this file.
    exit /b 1
  )
)
if not exist "%OUT%\bridge\bridge.config.json" (
  copy /y "bgi-bridge\bridge.config.example.json" "%OUT%\bridge\bridge.config.json" >nul
  if errorlevel 1 exit /b 1
)
xcopy /e /i /y "plugins\bgi" "%OUT%\plugins\bgi" >nul
if errorlevel 1 exit /b 1

echo [4/4] Building installer...
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\pack-payload.ps1"
if errorlevel 1 exit /b 1
"%CARGO%" build --release --features setup --bin sleepy-doll-setup
if errorlevel 1 exit /b 1
set "SETUP=%ROOT%\dist\Sleepy-Doll-%VERSION%-setup.exe"
copy /y "target\release\sleepy-doll-setup.exe" "%SETUP%" >nul
if errorlevel 1 exit /b 1

echo Done: %OUT%
echo Installer: %SETUP%
endlocal
