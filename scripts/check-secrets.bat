@echo off
setlocal enabledelayedexpansion
title ProjectRem - check Worker secrets

rem ===========================================================================
rem  Lists the Worker secrets that are not set yet.
rem
rem    check-secrets.bat [config-dir]
rem
rem  Called by push.bat; not meant to be run alone. `config-dir` defaults to
rem  `worker`.
rem
rem  `wrangler secret list` prints JSON, so the list is fetched once and each
rem  name is looked up in it, rather than re-running wrangler six times.
rem ===========================================================================

set "CFG_DIR=%~1"
if not defined CFG_DIR set "CFG_DIR=worker"

set "LISTING="
for /f "usebackq delims=" %%l in (`wrangler secret list -c "%CFG_DIR%\wrangler.toml" 2^>nul`) do set "LISTING=!LISTING!%%l "

if not defined LISTING (
    echo [push] WARNING: could not read the secret list - is CLOUDFLARE_API_TOKEN valid?
    exit /b 0
)

set "MISSING_COUNT=0"
for %%s in (HF_CLIENT_ID HF_CLIENT_SECRET DC_CLIENT_ID DC_CLIENT_SECRET GH_CLIENT_ID GH_CLIENT_SECRET) do (
    echo !LISTING! | findstr /c:"\"name\": \"%%s\"" >nul
    if errorlevel 1 (
        echo [push]   MISSING: %%s  ^(wrangler secret put %%s^)
        set /a MISSING_COUNT+=1
    )
)

if "!MISSING_COUNT!"=="0" (
    echo [push]   all six OAuth secrets are set.
) else (
    echo [push]   !MISSING_COUNT! secret^(s^) missing - OAuth will fail until they are set.
)

endlocal
exit /b 0
