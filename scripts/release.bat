@echo off
setlocal enabledelayedexpansion
title ProjectRem - publish release

rem ===========================================================================
rem  Tags the current version and creates a GitHub release with the packaged
rem  build attached. Called by build.bat; not meant to be run on its own.
rem
rem    release.bat <version> <previous-version>
rem
rem  This lives in its own file on purpose: build.bat calls it from inside a
rem  parenthesised block, where `call :label` would expand its arguments at
rem  parse time instead of at run time.
rem ===========================================================================

cd /d "%~dp0.."

set "NEW=%~1"
set "OLD=%~2"
set "OUT=Out\!NEW!"

if not defined NEW (
    echo [release] ERROR: no version was passed in.
    exit /b 1
)

echo [release] Publishing v!NEW! to GitHub ...

rem -------------------------------------------------------------- tooling
where git >nul 2>&1
if errorlevel 1 (
    echo [release] ERROR: git was not found on PATH.
    exit /b 1
)
where gh >nul 2>&1
if errorlevel 1 (
    echo [release] WARNING: the GitHub CLI was not found, skipping the release.
    echo [release]          Install it from https://cli.github.com to publish.
    exit /b 0
)

rem  gh reads the token from GH_TOKEN, GITHUB_TOKEN or its own config.
rem  It is never echoed.
if not defined GH_TOKEN if defined GITHUB_TOKEN set "GH_TOKEN=%GITHUB_TOKEN%"
if not defined GH_TOKEN (
    echo [release] WARNING: neither GH_TOKEN nor GITHUB_TOKEN is set, skipping the release.
    echo [release]          gh can also use a token from its own config.
    exit /b 0
)

rem  A release must be new. Refusing here beats gh overwriting one.
git rev-parse "!NEW!" >nul 2>&1
if not errorlevel 1 (
    echo [release] ERROR: the tag !NEW! already exists.
    echo [release]        Bump VERSION further, or delete the tag first.
    exit /b 1
)

rem ------------------------------------------------------------- asset
rem  A zip is what people actually download for a Windows desktop app.
rem
rem  The compression runs from a generated .ps1 rather than `-Command`: a long
rem  quoted one-liner is easy to break, and `powershell -Command` does not
rem  propagate a non-zero exit code, so a failure would be invisible.
set "ASSET="
if exist "!OUT!\" (
    set "ZIP=ProjectRem-!NEW!-windows-x64.zip"
    if exist "!ZIP!" del "!ZIP!"

    rem  Written with redirection rather than `echo` lines: cmd treats `(` and
    rem  `)` inside an echo as block delimiters, which mangles the script.
    set "PR_OUT=%CD%\!OUT!"
    set "PR_ZIP=%CD%\!ZIP!"
    set "PR_SCRIPT=%TEMP%\projectrem-zip-!NEW!.ps1"
    > "!PR_SCRIPT!" (
        echo $ErrorActionPreference = 'Stop'
        echo Compress-Archive -Path "$env:PR_OUT\*" -DestinationPath $env:PR_ZIP -Force
    )

    powershell -NoProfile -ExecutionPolicy Bypass -File "!PR_SCRIPT!" >nul
    del "!PR_SCRIPT!" >nul 2>&1

    if not exist "!ZIP!" (
        echo [release] ERROR: could not create the release archive.
        echo [release]        Check that "!OUT!" is readable and not locked.
        exit /b 1
    )
    set "ASSET=%CD%\!ZIP!"
    echo [release] Asset: !ZIP!
) else (
    echo [release] WARNING: "!OUT!" is missing, releasing without an asset.
)

rem -------------------------------------------------------------- publish
rem  The tag is pushed *before* the release is created. `--verify-tag` makes gh
rem  check the tag exists on the remote, and gh creates its own tag otherwise -
rem  which would not match the annotated local one.
git tag -a "!NEW!" -m "ProjectRem !NEW!"
if errorlevel 1 (
    echo [release] ERROR: could not create the tag !NEW!.
    exit /b 1
)

git push origin "!NEW!"
if errorlevel 1 (
    echo.
    echo [release] ERROR: could not push the tag !NEW!.
    echo [release]        Nothing was released. Fix the remote and re-run.
    exit /b 1
)

set "BODY=ProjectRem !NEW! ^(previous: !OLD!^). Windows x64 build. Download the zip, extract it, then run Start-ProjectRem.bat."

if defined ASSET (
    gh release create "!NEW!" "!ASSET!" --title "ProjectRem !NEW!" --notes "!BODY!" --verify-tag
) else (
    gh release create "!NEW!" --title "ProjectRem !NEW!" --notes "!BODY!" --verify-tag
)
if errorlevel 1 (
    echo.
    echo [release] WARNING: the tag !NEW! is pushed, but the release was not created.
    echo [release]          Create it by hand, or re-run this script.
    exit /b 0
)

echo [release] Release !NEW! is live:
echo [release]   https://github.com/TheNinoDev/ProjectRem/releases/tag/!NEW!
endlocal
exit /b 0
