@echo off
setlocal enabledelayedexpansion

rem BgiBridge build script.
rem   native/  -> MSVC (cl.exe), needs VS 2022 or Build Tools with the C++ workload
rem   managed/ -> dotnet SDK
rem
rem Everything lands under the repository's target/, next to Cargo's own output.
rem The injector uses absolute paths, so nothing is ever written into the
rem BetterGI install directory.
rem
rem NOTE: keep this file ASCII-only. cmd.exe reads .cmd in the OEM codepage and
rem non-ASCII text corrupts command parsing on non-UTF8 locales.

set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "DIST=%ROOT%\..\target\bridge"
set "SCRATCH=%ROOT%\..\target\dotnet"
set "NATIVE=%ROOT%\native"
set "MANAGED=%ROOT%\managed"

rem An existing bridge.config.json contains credentials and group settings and is
rem preserved. The tool-root config is only a seed for the first build.
set "CFG=%ROOT%\bridge.config.json"

echo [1/4] Locating MSVC...
set "VCVARS="
for %%P in (
  "%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
  "%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
  "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
  "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
  "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) do (
  if not defined VCVARS if exist %%P set "VCVARS=%%~P"
)
if not defined VCVARS (
  echo    vcvars64.bat not found. Install Visual Studio 2022 or Build Tools with C++.
  exit /b 1
)
echo    %VCVARS%
call "%VCVARS%" >nul
if errorlevel 1 (
  echo    vcvars64 initialization failed.
  exit /b 1
)

if not exist "%DIST%" mkdir "%DIST%"
if errorlevel 1 exit /b 1
if not exist "%SCRATCH%" mkdir "%SCRATCH%"
if errorlevel 1 exit /b 1

echo [2/4] Building native bootstrap DLL...
pushd "%NATIVE%"
cl /nologo /utf-8 /std:c++17 /O2 /MT /LD /EHsc /W3 /guard:cf /Fo"%DIST%\\" /Fe"%DIST%\BgiBridge.Bootstrap.dll" bootstrap.cpp /link /INCREMENTAL:NO
set "RC=%errorlevel%"
popd
if not "%RC%"=="0" (
  echo    bootstrap.dll build failed.
  exit /b 1
)

echo [3/4] Building injector...
pushd "%NATIVE%"
cl /nologo /utf-8 /std:c++17 /O2 /MT /EHsc /W3 /guard:cf /Fo"%DIST%\\" /Fe"%DIST%\BgiBridge.Injector.exe" injector.cpp /link /INCREMENTAL:NO Advapi32.lib Shell32.lib
set "RC=%errorlevel%"
popd
if not "%RC%"=="0" (
  echo    injector build failed.
  exit /b 1
)

echo [4/4] Building managed bridge...
rem Pin the output directory. Letting the SDK choose gives bin\<platform>\Release\...
rem and the platform segment varies with the environment - which once made this
rem script silently copy a stale DLL from a path the build no longer used.
set "MROOT=%SCRATCH%\managed"
pushd "%MANAGED%"
dotnet build -c Release -v quiet --nologo -o "%MROOT%"
set "RC=%errorlevel%"
popd
if not "%RC%"=="0" (
  echo    managed build failed.
  exit /b 1
)

set "MBIN=%MROOT%"
copy /y "%MBIN%\BgiBridge.dll" "%DIST%\" >nul
if errorlevel 1 exit /b 1
copy /y "%MBIN%\BgiBridge.runtimeconfig.json" "%DIST%\" >nul
if errorlevel 1 exit /b 1
copy /y "%MBIN%\BgiBridge.deps.json" "%DIST%\" >nul
if errorlevel 1 exit /b 1

echo Building offline recovery helper...
dotnet build "%ROOT%\recovery\Recovery.csproj" -c Release -v quiet --nologo -o "%DIST%"
if errorlevel 1 exit /b 1

rem Intermediate and debug-only files never ship.
del /q "%DIST%\*.obj" 2>nul
del /q "%DIST%\*.exp" 2>nul
del /q "%DIST%\*.lib" 2>nul
del /q "%DIST%\*.pdb" 2>nul

if not exist "%DIST%\bridge.config.json" (
  if not exist "%CFG%" copy /y "%ROOT%\bridge.config.example.json" "%CFG%" >nul
  copy /y "%CFG%" "%DIST%\bridge.config.json" >nul
  if errorlevel 1 exit /b 1
)

echo.
echo Build complete: %DIST%
dir /b "%DIST%"
endlocal
