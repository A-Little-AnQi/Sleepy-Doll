@echo off
rem Build the zzz native reflection bridge with MSVC C++20.
rem Outputs (and intermediate .obj files) stay in target\bridge\zzz.
rem Usage:
rem   build.cmd          build bridge DLL, injector EXE and pure tests EXE
rem   build.cmd --test   build ONLY the pure tests EXE; it is not executed here
rem                      (the isolated native runner is responsible for running)
rem
rem The test target links only zzz_pure + tests: no injector.cpp, no process
rem or socket host interfaces in the test binary.

setlocal enabledelayedexpansion

set "ROOT=%~dp0..\..\.."
for %%I in ("%ROOT%") do set "ROOT=%%~fI"
set "OUT=%ROOT%\target\bridge\zzz"
set "OBJ=%OUT%\obj"
set "WORK=%ROOT%\target\.tmp\local-adapters"

if not exist "%OUT%" mkdir "%OUT%"
if not exist "%OBJ%" mkdir "%OBJ%"
if not exist "%WORK%" mkdir "%WORK%" 2>nul

rem Keep tool temporary files inside the project, never the system Temp.
set "TEMP=%WORK%"
set "TMP=%WORK%"

set "TESTONLY="
if "%~1"=="--test" set "TESTONLY=1"

rem Locate MSVC: prefer the known local vcvars, fall back to vswhere.
set "VCVARS=E:\Programs\Mincrosoft\VisualStudio\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
    set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
    if not exist "!VSWHERE!" set "VSWHERE=%ProgramFiles%\Microsoft Visual Studio\Installer\vswhere.exe"
    if not exist "!VSWHERE!" (
        echo build.cmd: vswhere.exe not found. Install VS Build Tools 2022 with the C++ workload. 1>&2
        exit /b 1
    )
    set "VSPATH="
    for /f "usebackq delims=" %%I in (`"!VSWHERE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%I"
    if not defined VSPATH (
        echo build.cmd: no MSVC C++ toolchain found. Install the "Desktop development with C++" workload. 1>&2
        exit /b 1
    )
    set "VCVARS=!VSPATH!\VC\Auxiliary\Build\vcvars64.bat"
)
call "%VCVARS%" >nul
if errorlevel 1 (
    echo build.cmd: vcvars64.bat failed. 1>&2
    exit /b 1
)

set "COMMON=/nologo /std:c++20 /EHsc /utf-8 /MT /W4 /DWIN32 /D_WINDOWS /DNOMINMAX /c"
set "LINKFLAGS=/nologo /dynamicbase /nxcompat /highentropyva"

if defined TESTONLY goto :build_tests

echo [zzz-bridge] compiling zzz_pure
cl %COMMON% /Fo"%OBJ%\zzz_pure.obj" "%~dp0zzz_pure.cpp"
if errorlevel 1 goto :fail

echo [zzz-bridge] compiling zzz_bridge.dll
cl %COMMON% /Fo"%OBJ%\zzz_core.obj" "%~dp0zzz_core.cpp"
if errorlevel 1 goto :fail
cl %COMMON% /Fo"%OBJ%\zzz_dll.obj" "%~dp0zzz_dll.cpp"
if errorlevel 1 goto :fail
link %LINKFLAGS% /DLL /OUT:"%OUT%\zzz_bridge.dll" "%OBJ%\zzz_pure.obj" "%OBJ%\zzz_core.obj" "%OBJ%\zzz_dll.obj" ws2_32.lib bcrypt.lib windowsapp.lib
if errorlevel 1 goto :fail

echo [zzz-bridge] compiling zzz_injector.exe
cl %COMMON% /Fo"%OBJ%\injector.obj" "%~dp0injector.cpp"
if errorlevel 1 goto :fail
link %LINKFLAGS% /OUT:"%OUT%\zzz_injector.exe" "%OBJ%\injector.obj" ws2_32.lib
if errorlevel 1 goto :fail

:build_tests
echo [zzz-bridge] compiling zzz_tests.exe (pure logic only, not executed here)
cl %COMMON% /Fo"%OBJ%\zzz_pure.obj" "%~dp0zzz_pure.cpp"
if errorlevel 1 goto :fail
cl %COMMON% /Fo"%OBJ%\tests.obj" "%~dp0tests.cpp"
if errorlevel 1 goto :fail
link %LINKFLAGS% /OUT:"%OUT%\zzz_tests.exe" "%OBJ%\zzz_pure.obj" "%OBJ%\tests.obj" bcrypt.lib windowsapp.lib
if errorlevel 1 goto :fail

echo [zzz-bridge] build complete: %OUT%
exit /b 0

:fail
echo build.cmd: build failed. 1>&2
exit /b 1
