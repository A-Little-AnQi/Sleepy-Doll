@echo off
setlocal
cd /d "%~dp0"
call bgi-bridge\build.cmd
if errorlevel 1 exit /b 1
call npm run check
if errorlevel 1 exit /b 1
cargo build --release
if errorlevel 1 exit /b 1
for %%F in (BgiBridge.Injector.exe BgiBridge.Bootstrap.dll BgiBridge.dll BgiBridge.runtimeconfig.json BgiBridge.deps.json BgiBridge.Recovery.exe BgiBridge.Recovery.dll BgiBridge.Recovery.runtimeconfig.json BgiBridge.Recovery.deps.json) do (
  copy /y "bgi-bridge\dist\%%F" "target\release\" >nul
  if errorlevel 1 exit /b 1
)
echo Ready: target\release\sleepy-doll.exe
