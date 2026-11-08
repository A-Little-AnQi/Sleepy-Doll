@echo off
setlocal enabledelayedexpansion

rem MaaEnd native bridge build script.
rem   default    : maaend-bridge.exe (production server) + maaend-bridge-tests.exe
rem   build.cmd --test : only maaend-bridge-tests.exe (pure core header + fake
rem                transport; links neither WinHTTP nor WinSock, never launches
rem                or loads any target software)
rem Output goes to <repo>\target\bridge\maaend.
rem NOTE: keep this file ASCII-only. cmd.exe reads .cmd in the OEM codepage and
rem non-ASCII text corrupts command parsing on non-UTF8 locales.

set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
for %%I in ("%ROOT%\..\..\..") do set "REPO=%%~fI"
set "OUT=%REPO%\target\bridge\maaend"
set "TEMP=%REPO%\target\.tmp\local-adapters"
set "TMP=%TEMP%"
if not exist "%TEMP%" mkdir "%TEMP%"
if errorlevel 1 exit /b 1
if not exist "%OUT%" mkdir "%OUT%"
if errorlevel 1 exit /b 1

set "BUILD_TEST_ONLY=0"
if /i "%~1"=="--test" set "BUILD_TEST_ONLY=1"

echo [1/3] Locating MSVC...
set "VCVARS="
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%P in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
    if not defined VCVARS if exist "%%P\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%P\VC\Auxiliary\Build\vcvars64.bat"
  )
)
if not defined VCVARS (
  for %%P in (
    "E:\Programs\Mincrosoft\VisualStudio\VC\Auxiliary\Build\vcvars64.bat"
    "%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
    "%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
    "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
    "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
    "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
  ) do (
    if not defined VCVARS if exist %%P set "VCVARS=%%~P"
  )
)
if not defined VCVARS (
  echo    vcvars64.bat not found. Install VS 2022 or Build Tools with the C++ workload.
  exit /b 1
)
echo    %VCVARS%
call "%VCVARS%" >nul
if errorlevel 1 (
  echo    vcvars64 initialization failed.
  exit /b 1
)

set "COMMON=/nologo /utf-8 /std:c++20 /EHsc /W3 /guard:cf /DWINRT_LEAN_AND_MEAN"

if "%BUILD_TEST_ONLY%"=="1" goto tests

echo [2/3] Building maaend-bridge.exe ...
cl %COMMON% /O2 /MT "%ROOT%\maaend-bridge.cpp" /Fo"%OUT%\\" /Fe"%OUT%\maaend-bridge.exe" /link /INCREMENTAL:NO windowsapp.lib winhttp.lib ws2_32.lib bcrypt.lib
if errorlevel 1 (
  echo    maaend-bridge.exe build failed.
  exit /b 1
)
goto tests

:tests
echo [3/3] Building maaend-bridge-tests.exe ...
rem Tests include only the pure core header with an in-process fake transport:
rem no WinHTTP, no WinSock, no process or input APIs.
cl %COMMON% /O2 /MT "%ROOT%\tests.cpp" /Fo"%OUT%\\" /Fe"%OUT%\maaend-bridge-tests.exe" /link /INCREMENTAL:NO windowsapp.lib bcrypt.lib
if errorlevel 1 (
  echo    maaend-bridge-tests.exe build failed.
  exit /b 1
)

echo.
echo Build complete: %OUT%
dir /b "%OUT%"
endlocal
