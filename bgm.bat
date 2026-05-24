@echo off
REM bgm.bat - Windows command hub for BGM (mirror of scripts/bgm.sh).
REM Run everything through here so nothing leaks into your shell environment:
REM a project-local .bgm.env at the repo root is loaded only for the spawned
REM process.
REM
REM Usage:
REM   scripts\bgm.bat <url|flags...>     run a load test (default)
REM   scripts\bgm.bat run <url|flags...> explicit run
REM   scripts\bgm.bat example <name>     run examples\<name>.yml
REM   scripts\bgm.bat examples           list bundled examples
REM   scripts\bgm.bat build|fmt|clippy|test|check|clean|help
REM
REM Environment:
REM   BGM_PROFILE  cargo profile: release (default) or debug.

setlocal enableextensions enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "ROOT=%%~fI"
pushd "%ROOT%"

if not defined BGM_PROFILE set "BGM_PROFILE=release"
if /i "%BGM_PROFILE%"=="release" (
  set "CARGO_FLAGS=--release"
  set "BIN=%ROOT%\target\release\bgm.exe"
) else if /i "%BGM_PROFILE%"=="debug" (
  set "CARGO_FLAGS="
  set "BIN=%ROOT%\target\debug\bgm.exe"
) else (
  echo bgm.bat: unknown BGM_PROFILE "%BGM_PROFILE%" ^(use release^|debug^) 1>&2
  popd & exit /b 2
)

set "CMD=%~1"
set "ALL=%*"
if "%CMD%"=="" set "CMD=run"

if /i "%CMD%"=="build"    ( call :build & goto :done )
if /i "%CMD%"=="fmt"      ( cargo fmt --all & goto :done )
if /i "%CMD%"=="clippy"   ( cargo clippy --workspace --all-targets & goto :done )
if /i "%CMD%"=="test"     ( cargo test --workspace & goto :done )
if /i "%CMD%"=="check"    ( cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace & goto :done )
if /i "%CMD%"=="clean"    ( cargo clean & goto :done )
if /i "%CMD%"=="examples" ( call :examples & goto :done )
if /i "%CMD%"=="help"     ( call :usage & goto :done )
if /i "%CMD%"=="-h"       ( call :usage & goto :done )
if /i "%CMD%"=="--help"   ( call :usage & goto :done )

if /i "%CMD%"=="example" (
  if "%~2"=="" ( echo usage: bgm.bat example ^<name^> 1>&2 & popd & exit /b 2 )
  call :ensure_built
  call :load_env
  "%BIN%" --file "examples\%~2.yml"
  goto :done
)

if /i "%CMD%"=="run" (
  call :ensure_built
  call :load_env
  set "RARGS=!ALL:~3!"
  "%BIN%" !RARGS!
  goto :done
)

REM Default: treat every argument as run arguments (e.g. a URL).
call :ensure_built
call :load_env
"%BIN%" %ALL%
goto :done

:build
echo bgm.bat: building (%BGM_PROFILE%)... 1>&2
cargo build %CARGO_FLAGS% --bin bgm
goto :eof

:ensure_built
if not exist "%BIN%" call :build
goto :eof

:load_env
if exist "%ROOT%\.bgm.env" (
  for /f "usebackq eol=# tokens=1,* delims==" %%A in ("%ROOT%\.bgm.env") do set "%%A=%%B"
)
goto :eof

:examples
echo bgm.bat: available examples: 1>&2
for %%F in ("%ROOT%\examples\*.yml") do echo   %%~nF
goto :eof

:usage
echo BGM - run load tests via the project hub.
echo.
echo   scripts\bgm.bat ^<url^|flags...^>     run a load test (default)
echo   scripts\bgm.bat run ^<url^|flags...^> explicit run
echo   scripts\bgm.bat example ^<name^>     run examples\^<name^>.yml
echo   scripts\bgm.bat examples           list bundled examples
echo   scripts\bgm.bat build^|fmt^|clippy^|test^|check^|clean^|help
echo.
echo Environment: BGM_PROFILE = release ^(default^) ^| debug
goto :eof

:done
set "RC=%ERRORLEVEL%"
popd
exit /b %RC%
