@echo off
setlocal
cd /d "%~dp0"

rem One-command build. Produces dist\Sleepy-Doll\ with only the files that ship.
rem
rem The bridge components have to sit beside sleepy-doll.exe (bridge_control.rs
rem looks for them there), so they are copied next to it rather than left in
rem bgi-bridge\dist. Nothing goes into target\release - that directory belongs to
rem Cargo and is full of intermediate output.
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

echo [1/3] Building bridge components...
call bgi-bridge\build.cmd
if errorlevel 1 exit /b 1

echo [2/3] Building interface...
call npm run check
if errorlevel 1 exit /b 1

echo [3/3] Building desktop binary...
"%CARGO%" build --release
if errorlevel 1 exit /b 1

echo.
echo Assembling %OUT% ...
rem Overwrite in place rather than clearing the folder first. The bridge DLLs are
rem loaded inside a running BetterGI and cannot be deleted, so clearing first
rem leaves a half-destroyed install with no EXE. Unreplaced files are listed
rem individually and everything else stays usable.
if not exist "%OUT%" mkdir "%OUT%"
set "FAILED="

copy /y "target\release\sleepy-doll.exe" "%OUT%\" >nul
if errorlevel 1 set "FAILED=%FAILED% sleepy-doll.exe"

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
  copy /y "bgi-bridge\dist\%%F" "%OUT%\" >nul
  if errorlevel 1 set "FAILED=%FAILED% %%F"
)

rem Existing bridge.config.json contains the per-install authentication token.
rem Replacing it with the empty template breaks the next connection. Seed only
rem a brand-new package; normal rebuilds preserve the existing install config.
if not exist "%OUT%\bridge.config.json" (
  copy /y "bgi-bridge\bridge.config.example.json" "%OUT%\bridge.config.json" >nul
  if errorlevel 1 set "FAILED=%FAILED% bridge.config.json"
)

if defined FAILED (
  echo.
  echo    Could not replace:%FAILED%
  echo    Usually the bridge is still loaded in a running BetterGI. Exit BetterGI and
  echo    build again. BetterGI runs elevated, so an unelevated shell cannot stop it.
  echo    Everything else in the folder is up to date.
  exit /b 1
)

rem Skills that ship with the product. They live in the install folder, not under
rem user\ - user\ is the user's own data and is not touched by a rebuild.
if not exist "%OUT%\skills" mkdir "%OUT%\skills"
xcopy /e /i /y "skills" "%OUT%\skills" >nul
if errorlevel 1 (
  echo    could not copy skills\
  exit /b 1
)

rem An earlier layout dropped the package straight into dist\; those loose files
rem must not survive next to the product folder.
for %%F in ("%~dp0dist\*") do if /i not "%%~nxF"=="Sleepy-Doll" del /q "%%~fF" >nul 2>&1

echo.
echo Done: %OUT%
for %%F in ("%OUT%\*") do echo    %%~nxF
endlocal
