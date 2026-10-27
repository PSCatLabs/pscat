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
if /i "%~1"=="--hook" goto ps1
if /i "%~1"=="--locate" goto ps1
set "PSCAT_EXE="
for /f "usebackq delims=" %%i in (`powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" --exe ^<NUL`) do set "PSCAT_EXE=%%i"
if not defined PSCAT_EXE exit /b 1
"%PSCAT_EXE%" %*
exit /b %ERRORLEVEL%
:ps1
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0pscat-mcp.ps1" %* <NUL
exit /b %ERRORLEVEL%
