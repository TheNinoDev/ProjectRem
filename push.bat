@echo off
setlocal enabledelayedexpansion
title ProjectRem - push

rem ===========================================================================
rem  ProjectRem push script
rem
rem  1. Loads .env into the environment (the one place credentials live)
rem  2. Makes the Cloudflare Worker ready: namespace id is stamped in, then
rem     `wrangler deploy`
rem  3. Pushes the repository code to GitHub - and *only* the repository code.
rem     The Worker source, the built binary, the output folders and every
rem     credential are excluded by .gitignore, so they cannot reach the remote.
rem
rem  Flags:
rem    --no-worker   skip the Worker deploy
rem    --no-git      skip the git push
rem    --no-pause    do not pause at the end (for CI)
rem ===========================================================================

cd /d "%~dp0"

set "DEPLOY_WORKER=1"
set "PUSH_GIT=1"
for %%a in (%*) do (
    if /i "%%a"=="--no-worker" set "DEPLOY_WORKER=0"
    if /i "%%a"=="--no-git" set "PUSH_GIT=0"
)

echo.
echo ==========================================================
echo   ProjectRem push
echo ==========================================================
echo.

rem --------------------------------------------------------------- .env
rem  Nothing is hard-coded here: the Worker reads its secrets from
rem  `wrangler secret`, and these variables only tell wrangler *which*
rem  Cloudflare account to talk to and which namespace to use.
if exist ".env" (
    echo [push] Loading .env ...
    for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do (
        if not "%%a"=="" set "%%a=%%b"
    )
) else (
    echo [push] WARNING: no .env found. Cloudflare and GitHub steps may fail.
    echo [push]          Copy .env.example to .env and fill it in.
)
echo.

rem ------------------------------------------------------- sanity checks
where cargo >nul 2>&1
if errorlevel 1 (
    echo [push] ERROR: cargo was not found on PATH.
    goto :fail
)

set "MISSING=0"
if "!DEPLOY_WORKER!"=="1" (
    where wrangler >nul 2>&1
    if errorlevel 1 (
        echo [push] ERROR: wrangler was not found on PATH, so the Worker cannot be deployed.
        echo [push]        Install it with: npm i -g wrangler
        set "MISSING=1"
    )
    if not defined CLOUDFLARE_API_TOKEN (
        echo [push] ERROR: CLOUDFLARE_API_TOKEN is not set, so wrangler cannot authenticate.
        set "MISSING=1"
    )
    if not defined CF_KV_NAMESPACE_ID (
        rem  wrangler.toml already carries the development namespace, so this
        rem  is only fatal when the account differs.
        echo [push] Note: CF_KV_NAMESPACE_ID is not set, using the id in worker\wrangler.toml.
    )
)
if "!PUSH_GIT!"=="1" (
    where git >nul 2>&1
    if errorlevel 1 (
        echo [push] ERROR: git was not found on PATH.
        set "MISSING=1"
    )
)
if "!MISSING!"=="1" goto :fail

rem -------------------------------------------------- cloudflare worker
rem  The config path is passed explicitly with `-c` rather than relying on
rem  `pushd worker`: wrangler resolves `main` and the assets directory relative
rem  to the *config file*, but a `pushd` inside a parenthesised block is not
rem  always visible to the child process. Naming the file removes the
rem  dependency on the working directory entirely.
if "!DEPLOY_WORKER!"=="1" (
    echo [push] Preparing the Cloudflare Worker ...

    rem  Stamp the namespace id in, so the committed file never carries a
    rem  value from a different account.
    if defined CF_KV_NAMESPACE_ID (
        set "PR_WRANGLER=%CD%\worker\wrangler.toml"
        powershell -NoProfile -Command ^
            "$p=$env:PR_WRANGLER; $t=[System.IO.File]::ReadAllText($p); $t=[regex]::Replace($t,'(?m)^id\s*=\s*\"[0-9a-f]+\"$','id = \"' + $env:CF_KV_NAMESPACE_ID + '\"'); [System.IO.File]::WriteAllText($p,$t,(New-Object System.Text.UTF8Encoding($false)))" >nul
        if errorlevel 1 (
            echo [push] ERROR: could not write the namespace id into wrangler.toml.
            goto :fail
        )
        echo [push]   namespace: !CF_KV_NAMESPACE_ID!
    )

    echo [push] Deploying the Worker ...
    wrangler deploy -c "worker\wrangler.toml"
    if errorlevel 1 (
        echo.
        echo [push] ERROR: the Worker deploy failed.
        goto :fail
    )

    rem  A deploy is only "ready" if the secrets are set. Report what is
    rem  missing rather than failing at the first OAuth click.
    echo [push] Checking Worker secrets ...
    rem  Called from inside a parenthesised block, so it has to be a script.
    call "%~dp0scripts\check-secrets.bat" "worker"
    echo.
)

rem ------------------------------------------------------------- git
if "!PUSH_GIT!"=="1" (
    echo [push] Checking what would be published ...
    echo.
    rem  A guard rather than a convenience: if a credential ever shows up in
    rem  the index, this is the last chance to catch it.
    git status --porcelain
    echo.

    git check-ignore -q .env
    if errorlevel 1 (
        echo [push] WARNING: .env is NOT ignored by git. Add it to .gitignore
        echo [push]          before committing, or your credentials will leak.
    )

    git add -A
    if errorlevel 1 (
        echo [push] ERROR: git add failed.
        goto :fail
    )

    rem  Refuse to commit a secret that slipped past .gitignore.
    git diff --cached --name-only | findstr /i /c:"^\.env" >nul
    if not errorlevel 1 (
        echo.
        echo [push] ERROR: a .env file is staged. Remove it from the index:
        echo [push]          git rm --cached .env
        echo [push]        Nothing was pushed.
        goto :fail
    )

    git diff --cached --quiet
    if errorlevel 1 (
        rem  Use the newest commit subject as the message, so a push right
        rem  after a working session reads sensibly.
        for /f "delims=" %%m in ('powershell -NoProfile -Command "git log -1 --pretty=format:'%%s' 2^>$null"') do set "MSG=%%m"
        if not defined MSG set "MSG=ProjectRem update"
        git commit -m "!MSG!"
        if errorlevel 1 (
            echo [push] ERROR: the commit failed.
            goto :fail
        )
    ) else (
        echo [push] Nothing to commit, the tree is already clean.
    )

    rem  `--set-upstream` covers the very first push, where no tracking branch
    rem  exists yet and a bare `git push` refuses.
    git push --set-upstream origin HEAD
    if errorlevel 1 (
        echo.
        echo [push] ERROR: git push failed. Check your remote and credentials.
        goto :fail
    )
    echo [push] Pushed to origin.
)

echo.
echo ==========================================================
echo   Push finished
echo ==========================================================
echo.
if not "%~1"=="--no-pause" pause
endlocal
exit /b 0

:fail
echo.
echo [push] PUSH FAILED.
if not "%~1"=="--no-pause" pause
endlocal
exit /b 1
