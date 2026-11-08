@echo off
rem March7th native bridge build (ASCII only).
rem Usage:
rem   build.cmd            build DLL + injector + tests (no execution)
rem   build.cmd --test     build only the test executable (no execution)
rem Output goes to target\bridge\march7th (exe/dll/pdb/obj).
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..\..") do set "REPO_ROOT=%%~fI"
set "OUT_DIR=%REPO_ROOT%\target\bridge\march7th"
set "TMP_DIR=%REPO_ROOT%\target\.tmp\local-adapters"
if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"
if not exist "%TMP_DIR%" mkdir "%TMP_DIR%"
set "TEMP=%TMP_DIR%"
set "TMP=%TMP_DIR%"
set "TMPDIR=%TMP_DIR%"
set "PYTHONDONTWRITEBYTECODE=1"

set "BUILD_ALL=1"
if /i "%~1"=="--test" set "BUILD_ALL=0"

rem Locate MSVC x64 toolchain: vswhere first, then known fallback path.
set "VCVARS="
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
    for /f "usebackq delims=" %%P in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
        if exist "%%P\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%P\VC\Auxiliary\Build\vcvars64.bat"
    )
)
if not defined VCVARS (
    if exist "E:\Programs\Mincrosoft\VisualStudio\VC\Auxiliary\Build\vcvars64.bat" (
        set "VCVARS=E:\Programs\Mincrosoft\VisualStudio\VC\Auxiliary\Build\vcvars64.bat"
    )
)
if not defined VCVARS (
    echo build.cmd: MSVC x64 toolchain not found via vswhere or fallback path 1>&2
    exit /b 1
)

call "%VCVARS%" >nul
if errorlevel 1 exit /b 1

set "COMMON=/nologo /std:c++20 /utf-8 /EHsc /W4 /O2 /MT /D_WIN32_WINNT=0x0A00 /DWIN32_LEAN_AND_MEAN /DNOMINMAX /DUNICODE /D_UNICODE"
set "SRC=%SCRIPT_DIR%"

if "%BUILD_ALL%"=="1" (
    rem Bridge DLL: reflection core + HTTP transport + pipe control plane entry.
    cl %COMMON% /LD /Fo"%OUT_DIR%\\" /Fd"%OUT_DIR%\march7th_bridge.pdb" ^
        /Fe"%OUT_DIR%\march7th_bridge.dll" ^
        "%SRC%core.cpp" "%SRC%routing.cpp" "%SRC%host_python.cpp" "%SRC%http_server.cpp" "%SRC%dll_main.cpp" ^
        /link /DLL windowsapp.lib ws2_32.lib advapi32.lib /OUT:"%OUT_DIR%\march7th_bridge.dll"
    if errorlevel 1 exit /b 1

    rem Injector exe: attaches the DLL to an already running target process.
    cl %COMMON% /Fo"%OUT_DIR%\injector.obj" /Fe"%OUT_DIR%\march7th_injector.exe" ^
        "%SRC%injector.cpp" /link /OUT:"%OUT_DIR%\march7th_injector.exe"
    if errorlevel 1 exit /b 1
)

rem Tests exe: pure core + routing + fake host. Never links host_python,
rem http_server or injector, so the binary carries no injection, process,
rem socket or input imports. Built but not executed here.
cl %COMMON% /Fo"%OUT_DIR%\\" /Fe"%OUT_DIR%\march7th-bridge-tests.exe" ^
    "%SRC%tests.cpp" "%SRC%core.cpp" "%SRC%routing.cpp" ^
    /link /OUT:"%OUT_DIR%\march7th-bridge-tests.exe" windowsapp.lib
if errorlevel 1 exit /b 1

echo build.cmd: outputs in %OUT_DIR%
exit /b 0
