@echo off
rem pscat plugin launcher for Windows (issue #866). The plugin manifests name
rem scripts\pscat-mcp with no extension; on Windows the hosts find this file
rem beside it (Claude Code through cross-spawn, Codex through the which crate,
rem PowerShell by PATHEXT). pscat-mcp.ps1 finds or installs pscat; then this
rem file runs pscat-mcp.exe itself, so PowerShell is not in the MCP stdio
rem path. stdout is the MCP channel: nothing here writes to it.
setlocal
if defined PSCAT_PLUGIN_LAUNCHER (
  echo pscat plugin: PSCAT_MCP_BIN must name pscat-mcp itself, not this launcher 1>&2
  exit /b 1
)
rem Windows PowerShell rebuilds its own module path; one inherited from
rem PowerShell 7 (a host started from pwsh) can hide its modules.
set "PSModulePath="
if /i "%~1"=="--hook" goto ps1
if /i "%~1"=="--locate" goto ps1
rem --exe prints one line TAG|... for the program (E), one for the output
rem root if there is one (R), and A|drop when this file was given only the
rem launcher's own --plugin-* options (they are not passed on), else A|keep.
rem A program or root is an environment variable name, a bar and the rest of
rem the path below that variable's directory (so only the ASCII rest goes
rem through the console code page and any user name survives), or a dash, a
rem bar and the whole path.
set "PSCAT_BASE="
set "PSCAT_REL="
set "PSCAT_RBASE="
set "PSCAT_RREL="
set "PSCAT_ARGS="
for /f "usebackq tokens=1,2,* delims=|" %%a in (`powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" --exe %* ^<NUL`) do (
  if "%%a"=="E" (
    set "PSCAT_BASE=%%b"
    set "PSCAT_REL=%%c"
  )
  if "%%a"=="R" (
    set "PSCAT_RBASE=%%b"
    set "PSCAT_RREL=%%c"
  )
  if "%%a"=="A" set "PSCAT_ARGS=%%b"
)
if not defined PSCAT_REL exit /b 1
if not "%PSCAT_BASE%"=="-" goto based
set "PSCAT_EXE=%PSCAT_REL%"
goto root
:based
rem CALL expands the line twice: the first pass leaves the variable's
rem reference, the second its value.
call set "PSCAT_EXE=%%%PSCAT_BASE%%%\%PSCAT_REL%"
:root
set "PSCAT_ROOT="
if not defined PSCAT_RREL goto run
if not "%PSCAT_RBASE%"=="-" goto rbased
set "PSCAT_ROOT=%PSCAT_RREL%"
goto run
:rbased
call set "PSCAT_ROOT=%%%PSCAT_RBASE%%%\%PSCAT_RREL%"
:run
if /i "%PSCAT_ARGS%"=="drop" goto rundrop
if defined PSCAT_ROOT "%PSCAT_EXE%" --out-root "%PSCAT_ROOT%" %*
if not defined PSCAT_ROOT "%PSCAT_EXE%" %*
exit /b %ERRORLEVEL%
:rundrop
if defined PSCAT_ROOT "%PSCAT_EXE%" --out-root "%PSCAT_ROOT%"
if not defined PSCAT_ROOT "%PSCAT_EXE%"
exit /b %ERRORLEVEL%
:ps1
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" %* <NUL
exit /b %ERRORLEVEL%
