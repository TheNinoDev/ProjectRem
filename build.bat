@echo off
setlocal enabledelayedexpansion
title ProjectRem - build

rem ===========================================================================
rem  ProjectRem build script
rem
rem  1. Bumps the version in the VERSION file (0.0.0 -> <inf>.10.10 scheme)
rem  2. Runs the test suite
rem  3. Builds the Rust GUI in release mode
rem  4. Creates a new folder in Out\ named after the new version index
rem  5. Copies the executable, the Python sidecar and the version into it
rem  6. Tags the commit and creates a GitHub release with the build as an asset
rem
rem  The VERSION file is the single source of truth for the version number, so
rem  the window title, the Settings -> About panel and the Python sidecar all
rem  pick up the new number automatically.
rem
rem  Flags:
rem    --no-tests    skip the test suite
rem    --no-release  build and package, but do not touch GitHub
rem    --no-pause    do not pause at the end (for CI)
rem ===========================================================================

cd /d "%~dp0"

set "RUN_TESTS=1"
set "RUN_RELEASE=1"
for %%a in (%*) do (
    if /i "%%a"=="--no-tests" set "RUN_TESTS=0"
    if /i "%%a"=="--no-release" set "RUN_RELEASE=0"
)

echo.
echo ==========================================================
echo   ProjectRem build
echo ==========================================================
echo.

rem ------------------------------------------------------------- prerequisites
where cargo >nul 2>&1
if errorlevel 1 (
    echo [build] ERROR: cargo was not found on PATH.
    echo [build]        Install Rust from https://rustup.rs and open a new console.
    goto :fail
)

if not exist "VERSION" (
    echo [build] ERROR: the VERSION file is missing from %CD%
    goto :fail
)

rem ------------------------------------------------------------- version bump
rem  Scheme: major is unbounded, minor and patch run from 0 to 10.
rem  0.0.0 -^> 0.0.1 -^> ... -^> 0.0.10 -^> 0.1.0 -^> ... -^> 0.10.10 -^> 1.0.0 -^> ...
set "OLD_VERSION="
for /f "usebackq delims=" %%v in ("VERSION") do set "OLD_VERSION=%%v"

echo [build] Current version: !OLD_VERSION!

set "VERSION="
for /f "usebackq delims=" %%v in (`powershell -NoProfile -ExecutionPolicy Bypass -Command "$p='VERSION'; $n=((Get-Content -LiteralPath $p -Raw).Trim() -split '\.'); if($n.Count -ne 3){ Write-Error 'VERSION must look like 0.0.0'; exit 1 }; $maj=[int]$n[0]; $mi=[int]$n[1]; $pa=[int]$n[2]; if($pa -lt 10){ $pa=$pa+1 } else { $pa=0; if($mi -lt 10){ $mi=$mi+1 } else { $mi=0; $maj=$maj+1 } }; $new='{0}.{1}.{2}' -f $maj,$mi,$pa; Set-Content -LiteralPath $p -Value $new -NoNewline -Encoding ascii; Write-Output $new"`) do set "VERSION=%%v"

if not defined VERSION (
    echo [build] ERROR: could not bump the version.
    goto :fail
)

if not defined OLD_VERSION set "OLD_VERSION=0.0.0"
echo [build] New version:     !VERSION!
echo.

rem ------------------------------------------------------------------- tests
if "!RUN_TESTS!"=="1" (
    echo [build] Running the test suite...
    echo.
    cargo test --workspace
    if errorlevel 1 (
        echo.
        echo [build] ERROR: tests failed. Nothing was published.
        goto :fail
    )
    echo.
)

rem ------------------------------------------------------------------- build
echo [build] Compiling the release build, this takes a while on the first run...
echo.
cargo build --release --workspace
if errorlevel 1 (
    echo.
    echo [build] ERROR: cargo build failed. The version in VERSION is now !VERSION!
    goto :fail
)

rem  The target directory lives at the workspace root (next to this script),
rem  not inside main\, because Cargo.toml here is a virtual workspace.
set "EXE=%CD%\target\release\projectrem-gui.exe"

if not exist "%EXE%" (
    echo.
    echo [build] ERROR: "%EXE%" was not produced.
    goto :fail
)

rem ------------------------------------------------------------- output folder
set "OUTDIR=Out\!VERSION!"
if exist "!OUTDIR!" (
    echo [build] Note: "!OUTDIR!" already exists, overwriting it.
)

if not exist "Out" mkdir "Out"
if not exist "!OUTDIR!" mkdir "!OUTDIR!"

echo [build] Packaging into "!OUTDIR!" ...
copy /y "%EXE%" "!OUTDIR!\" >nul
if errorlevel 1 (
    echo [build] ERROR: could not copy the executable.
    goto :fail
)

xcopy /e /i /q /y "sidecar" "!OUTDIR!\sidecar\" >nul
if errorlevel 1 (
    echo [build] ERROR: could not copy the Python sidecar.
    goto :fail
)

copy /y "VERSION" "!OUTDIR!\" >nul
if exist "README.md" copy /y "README.md" "!OUTDIR!\" >nul

rem A launcher so the built folder is self-contained.
(
    echo @echo off
    echo cd /d "%%~dp0"
    echo start "" "%%~dp0projectrem-gui.exe"
) > "!OUTDIR!\Start-ProjectRem.bat"

rem -------------------------------------------------------------- build info
set "RUSTC_VERSION="
for /f "delims=" %%r in ('rustc --version 2^>nul') do set "RUSTC_VERSION=%%r"

(
    echo ProjectRem !VERSION!
    echo previous version: !OLD_VERSION!
    echo built:            %DATE% %TIME%
    echo rustc:            !RUSTC_VERSION!
    echo target:           windows-x86_64-pc-windows-msvc
    echo.
    echo contents:
    echo   projectrem-gui.exe    the GUI
    echo   Start-ProjectRem.bat  launcher
    echo   sidecar\main.py       Python sidecar
    echo   VERSION               version marker
) > "!OUTDIR!\build-info.txt"

rem ------------------------------------------------------- GitHub release
if "!RUN_RELEASE!"=="1" (
    rem  A plain `call` from inside a parenthesised block would expand its
    rem  variables at parse time, so the release runs in a separate script.
    call "%~dp0scripts\release.bat" "!VERSION!" "!OLD_VERSION!"
    if errorlevel 1 goto :fail
)

echo.
echo ==========================================================
echo   Build finished
echo ==========================================================
echo   Version: !VERSION!  (was !OLD_VERSION!)
echo   Output:  %CD%\!OUTDIR!
echo ==========================================================
echo.
if not "%~1"=="--no-pause" pause
endlocal
exit /b 0

rem ---------------------------------------------------------------------------
rem  Tag !1! and attach the packaged build to a GitHub release.
rem  Called with setlocal of its own so it cannot disturb the build state.
rem ---------------------------------------------------------------------------
:publish
setlocal
set "NEW=%~1"
set "OLD=%~2"
set "OUT=%~3"

echo [build] Publishing v!NEW! to GitHub ...

where git >nul 2>&1
if errorlevel 1 (
    echo [build] ERROR: git was not found on PATH.
    exit /b 1
)
where gh >nul 2>&1
if errorlevel 1 (
    echo [build] WARNING: the GitHub CLI was not found, skipping the release.
    echo [build]          Install it from https://cli.github.com to publish.
    exit /b 0
)

rem  gh reads the token from GH_TOKEN, GITHUB_TOKEN or its own config.
rem  Never echo it.
set "GH_TOKEN=%GH_TOKEN%"
if not defined GH_TOKEN set "GH_TOKEN=%GITHUB_TOKEN%"
if not defined GH_TOKEN (
    echo [build] WARNING: neither GH_TOKEN nor GITHUB_TOKEN is set, skipping the release.
    echo [build]          gh can also use a token from its own config.
    exit /b 0
)

rem  Refuse to overwrite an existing tag: a release must be new.
git rev-parse "!NEW!" >nul 2>&1
if not errorlevel 1 (
    echo [build] ERROR: the tag v!NEW! already exists.
    echo [build]        Bump VERSION further, or delete the tag first.
    exit /b 1
)

rem  A zip is what people actually download for a Windows desktop app.
if exist "!OUT!" (
    powershell -NoProfile -Command ^
        "Compress-Archive -LiteralPath '%CD%\!OUT!\*' -DestinationPath '%CD%\ProjectRem-!NEW!-windows-x64.zip' -Force" >nul
    if errorlevel 1 (
        echo [build] ERROR: could not create the release archive.
        exit /b 1
    )
    set "ASSET=%CD%\ProjectRem-!NEW!-windows-x64.zip"
) else (
    set "ASSET="
)

git tag -a "!NEW!" -m "ProjectRem !NEW!"
if errorlevel 1 (
    echo [build] ERROR: could not create the tag v!NEW!.
    exit /b 1
)

set "BODY=ProjectRem !NEW! ^(previous: !OLD!^).

Windows x64 build. Download the zip, extract it and run Start-ProjectRem.bat."

if defined ASSET (
    gh release create "!NEW!" "!ASSET!" --title "ProjectRem !NEW!" --notes "%BODY%" --verify-tag
) else (
    gh release create "!NEW!" --title "ProjectRem !NEW!" --notes "%BODY%" --verify-tag
)

if errorlevel 1 (
    echo.
    echo [build] WARNING: the release was not created.
    echo [build]          The tag v!NEW! was pushed locally - run
    echo [build]            git push origin !NEW!
    echo [build]          and create the release by hand.
    endlocal & exit /b 0
)

rem  Pushing the tag is what makes the release visible to everyone else.
git push origin "!NEW!"
if errorlevel 1 (
    echo [build] WARNING: could not push the tag. Run: git push origin !NEW!
    endlocal & exit /b 0
)

echo [build] Release v!NEW! is live.
endlocal & exit /b 0

:fail
echo.
echo [build] BUILD FAILED.
if not "%~1"=="--no-pause" pause
endlocal
exit /b 1