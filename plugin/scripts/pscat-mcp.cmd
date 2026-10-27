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
rem --exe prints an environment variable name, a bar and the rest of the
rem path below that variable's directory (so only the ASCII rest goes
rem through the console code page and any user name survives), or a dash,
rem a bar and the whole path.
set "PSCAT_BASE="
set "PSCAT_REL="
for /f "usebackq tokens=1,* delims=|" %%a in (`powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" --exe ^<NUL`) do (
  set "PSCAT_BASE=%%a"
  set "PSCAT_REL=%%b"
)
if not defined PSCAT_REL exit /b 1
if not "%PSCAT_BASE%"=="-" goto based
set "PSCAT_EXE=%PSCAT_REL%"
goto run
:based
rem CALL expands the line twice: the first pass leaves the variable's
rem reference, the second its value.
call set "PSCAT_EXE=%%%PSCAT_BASE%%%\%PSCAT_REL%"
:run
"%PSCAT_EXE%" %*
exit /b %ERRORLEVEL%
:ps1
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" %* <NUL
exit /b %ERRORLEVEL%
