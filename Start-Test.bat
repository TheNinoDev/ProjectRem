@echo off
setlocal enabledelayedexpansion
title ProjectRem - start test build

rem ===========================================================================
rem  ProjectRem development launcher
rem
rem  1. Checks that cargo and python are available
rem  2. Creates .venv\ and installs sidecar\requirements.txt the first time
rem  3. Runs the GUI with `cargo run`
rem
rem  The GUI always uses the version in the VERSION file. Use build.bat to
rem  bump the version and produce a release build in Out\<version>\.
rem ===========================================================================

cd /d "%~dp0"

echo.
echo ==========================================================
echo   ProjectRem - start test build
echo ==========================================================
echo.

if not exist "VERSION" (
    echo [start] ERROR: the VERSION file is missing from %CD%
    goto :fail
)
for /f "usebackq delims=" %%v in ("VERSION") do set "VERSION=%%v"
echo [start] Version: !VERSION!
echo.

rem ------------------------------------------------------------- prerequisites
where cargo >nul 2>&1
if errorlevel 1 (
    echo [start] ERROR: cargo was not found on PATH.
    echo [start]        Install Rust from https://rustup.rs and open a new console.
    goto :fail
)

set "PYTHON="
where py >nul 2>&1 && set "PYTHON=py -3"
if not defined PYTHON (
    where python >nul 2>&1 && set "PYTHON=python"
)
if not defined PYTHON (
    echo [start] WARNING: no python found. The GUI still starts, but the
    echo [start]          Python sidecar will be reported as unavailable.
    echo.
    goto :run
)

rem --------------------------------------------------------------- python venv
if exist ".venv\Scripts\python.exe" (
    set "VENV_PY=%CD%\.venv\Scripts\python.exe"
    echo [start] Using the existing virtual environment: .venv
) else (
    echo [start] Creating the virtual environment .venv ...
    %PYTHON% -m venv .venv >nul 2>&1
    if errorlevel 1 (
        echo [start] WARNING: could not create .venv, falling back to the system Python.
        goto :run
    )
    set "VENV_PY=%CD%\.venv\Scripts\python.exe"
    echo [start] Installing the sidecar dependencies, this can take a minute ...
    "!VENV_PY!" -m pip install --upgrade pip >nul 2>&1
    "!VENV_PY!" -m pip install -r "sidecar\requirements.txt" >nul 2>&1
    if errorlevel 1 (
        echo [start] WARNING: pip install failed. The GUI still works, it will
        echo [start]          just show "System default" as the only audio device.
    ) else (
        echo [start] Dependencies installed.
    )
)

set "PROJECTREM_PYTHON=!VENV_PY!"

:run
echo.
echo [start] Building and starting the GUI ...
echo.
cargo run --workspace

if errorlevel 1 (
    echo.
    echo [start] The app exited with an error.
    goto :fail
)

echo.
echo [start] Closed.
pause
endlocal
exit /b 0

:fail
echo.
echo [start] START FAILED.
pause
endlocal
exit /b 1