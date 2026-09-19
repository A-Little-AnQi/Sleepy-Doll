@echo off
setlocal
cd /d "%~dp0"

rem One-command build. Produces dist\Sleepy-Doll\ with only the files that ship.
rem
rem dist\Sleepy-Doll is the only product output; every intermediate lands under
rem target\ (target\bridge, target\dotnet, target\ui, target\release). The bridge
rem components are copied into a bridge\ subdirectory of the product (bridge\
rem control.rs looks for them there), so the product folder keeps the executable
rem on its own instead of carrying eleven bridge files beside it.
rem
rem The product folder is what gets handed out, so unzipping it yields a folder
rem instead of a pile of loose files.

set "OUT=%~dp0dist\Sleepy-Doll"

rem Cargo is not always on PATH: rustup installs to %CARGO_HOME%\bin, which can be
rem custom and may not be picked up by an already-running shell.
set "CARGO=cargo"
where cargo >nul 2>&1
if errorlevel 1 (
  set "CARGO="
  if exist "%CARGO_HOME%\bin\cargo.exe" set "CARGO=%CARGO_HOME%\bin\cargo.exe"
  if not defined CARGO if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
  if not defined CARGO (
    echo    cargo not found. Install Rust, or add %%CARGO_HOME%%\bin to PATH.
    exit /b 1
  )
  echo    using %CARGO%
)

rem The installer filename carries the package version, and the release workflow
rem refuses a tag that disagrees with Cargo.toml. Read it from the same place.
for /f "tokens=2 delims== " %%V in ('findstr /b /c:"version = " Cargo.toml') do set "VERSION=%%~V"
if not defined VERSION (
  echo    could not read package.version from Cargo.toml.
  exit /b 1
)

echo [1/4] Building bridge components...
call bgi-bridge\build.cmd
if errorlevel 1 exit /b 1

echo [2/4] Building interface...
call npm run build
if errorlevel 1 exit /b 1

echo [3/4] Building desktop binary...
"%CARGO%" build --release
if errorlevel 1 exit /b 1

echo.
echo Assembling %OUT% ...
rem Overwrite in place rather than clearing the folder first. The bridge DLLs are
rem loaded inside a running BetterGI and cannot be deleted, so clearing first
rem leaves a half-destroyed install with no EXE. Unreplaced files are listed
rem individually and everything else stays usable.
if not exist "%OUT%" mkdir "%OUT%"
if not exist "%OUT%\bridge" mkdir "%OUT%\bridge"
set "FAILED="

copy /y "target\release\sleepy-doll.exe" "%OUT%\" >nul
if errorlevel 1 set "FAILED=%FAILED% sleepy-doll.exe"

rem Earlier packages kept the bridge components flat beside the executable. Carry
rem the config over first - it holds the per-install token - then drop the stale
rem copies so the new layout is what the folder shows.
if exist "%OUT%\bridge.config.json" if not exist "%OUT%\bridge\bridge.config.json" (
  move /y "%OUT%\bridge.config.json" "%OUT%\bridge\bridge.config.json" >nul
)
if exist "%OUT%\BgiBridge.*" del /q "%OUT%\BgiBridge.*" >nul 2>&1

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
  if errorlevel 1 set "FAILED=%FAILED% bridge\%%F"
)

rem Existing bridge.config.json contains the per-install authentication token.
rem Replacing it with the empty template breaks the next connection. Seed only
rem a brand-new package; normal rebuilds preserve the existing install config.
if not exist "%OUT%\bridge\bridge.config.json" (
  copy /y "bgi-bridge\bridge.config.example.json" "%OUT%\bridge\bridge.config.json" >nul
  if errorlevel 1 set "FAILED=%FAILED% bridge.config.json"
)

rem Skills that ship with the product. They live in the install folder, not under
rem user\ - user\ is the user's own data and is not touched by a rebuild.
if not exist "%OUT%\skills" mkdir "%OUT%\skills"
xcopy /e /i /y "skills" "%OUT%\skills" >nul
if errorlevel 1 (
  echo    could not copy skills\
  exit /b 1
)

if defined FAILED (
  echo.
  echo    Could not replace:%FAILED%
  echo    Usually the bridge is still loaded in a running BetterGI. Exit BetterGI and
  echo    build again. BetterGI runs elevated, so an unelevated shell cannot stop it.
  echo    Everything else in the folder is up to date.
  exit /b 1
)

echo.
echo [4/4] Building installer...
rem The payload is the folder just assembled, so this has to run after it. The
rem setup binary embeds the packed result, which is why it is a separate feature:
rem an ordinary cargo build must not try to compile it.
powershell -NoProfile -ExecutionPolicy Bypass -File "installer\pack-payload.ps1"
if errorlevel 1 exit /b 1
"%CARGO%" build --release --features setup --bin sleepy-doll-setup
if errorlevel 1 exit /b 1
set "SETUP=%~dp0dist\Sleepy-Doll-%VERSION%-setup.exe"
copy /y "target\release\sleepy-doll-setup.exe" "%SETUP%" >nul
if errorlevel 1 (
  echo    could not write %SETUP%
  exit /b 1
)

echo.
echo Done: %OUT%
for %%F in ("%OUT%\*") do echo    %%~nxF
for %%F in ("%OUT%\bridge\*") do echo    bridge\%%~nxF
echo.
echo Done: %SETUP%
endlocal
