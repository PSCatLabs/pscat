# pscat plugin launcher for Windows (issue #866), the twin of pscat-mcp.sh:
# runs pscat-mcp for Claude Code, Cowork and Codex, installing pscat first if
# it is missing. Windows PowerShell 5.1 (in every Windows 10 and 11) or later.
#
#   pscat-mcp.cmd [pscat-mcp args]   what the hosts run: this script finds or
#                                    installs pscat (--exe), then the .cmd runs
#                                    pscat-mcp.exe itself, so PowerShell is not
#                                    in the MCP stdio path
#   pscat-mcp.ps1 --locate           find or install pscat, print its directory
#   pscat-mcp.ps1 --hook             SessionStart hook: report, never download
#   pscat-mcp.ps1 --exe              find or install, tell pscat-mcp.cmd the
#                                    path of pscat-mcp (see ExeHandoff)
#   pscat-mcp.ps1 --exe-utf8         the same as plain UTF-8 (for Git Bash)
#   pscat-mcp.ps1 [pscat-mcp args]   find or install, then run pscat-mcp
#
# Lookup, in order: $env:PSCAT_MCP_BIN, pscat-mcp on PATH, the Cargo bin
# directory. A candidate counts only if `pscat-mcp --version` is at least the
# version of the pinned release (the tag in ..\pscat-release, pre-release
# suffix dropped). Then the cached copy of the pinned release. Otherwise
# download that release's Windows .zip from GitHub, check it against the
# release's SHA256SUMS, unpack it into the cache and run it from there.
#
# Cache: $env:PSCAT_CACHE_DIR, else %LOCALAPPDATA%\pscat\plugin.
# $env:PSCAT_RELEASE_BASE_URL replaces
# https://github.com/PSCatLabs/pscat/releases/download (tests use a local
# HTTP server).
#
# stdout is the MCP stdio channel: everything this script says goes to
# stderr, except --locate's directory, --exe's path and --hook's JSON.
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RepoUrl = 'https://github.com/PSCatLabs/pscat'
$Here = $PSScriptRoot
$PluginDir = Split-Path -Parent $Here

function Say([string]$Msg) { [Console]::Error.WriteLine("pscat plugin: $Msg") }
function Die([string]$Msg) { Say $Msg; exit 1 }
# stdout as UTF-8 without a BOM, whatever the console code page is.
function Out-Utf8([string]$Text) {
    $bytes = (New-Object System.Text.UTF8Encoding $false).GetBytes($Text + "`n")
    $out = [Console]::OpenStandardOutput()
    $out.Write($bytes, 0, $bytes.Length)
    $out.Flush()
}

# PSCAT_MCP_BIN pointing back at the launcher would recurse forever.
if ($env:PSCAT_PLUGIN_LAUNCHER) { Die 'PSCAT_MCP_BIN must name pscat-mcp itself, not this launcher' }
$env:PSCAT_PLUGIN_LAUNCHER = '1'

# --- the pinned release ------------------------------------------------------
$Tag = ''
$pinFile = Join-Path $PluginDir 'pscat-release'
if (Test-Path -LiteralPath $pinFile -PathType Leaf) {
    foreach ($line in [IO.File]::ReadAllLines($pinFile)) {
        if ($line -match '^\s*#') { continue }
        $t = $line -replace '\s', ''
        if ($t) { $Tag = $t; break }
    }
}
if ($Tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+') { $Tag = '' }
# 0.7.0 from v0.7.0-rc.1: the lowest installed version this plugin accepts.
$MinVersion = ($Tag -replace '^v', '') -replace '[-+].*$', ''
$BaseUrl = $RepoUrl + '/releases/download'
if ($env:PSCAT_RELEASE_BASE_URL) { $BaseUrl = $env:PSCAT_RELEASE_BASE_URL }
$BaseUrl = $BaseUrl.TrimEnd('/')

# --- platform ------------------------------------------------------------------
# The releases have one Windows build, x86_64. Windows 11 on ARM runs it
# under emulation; 32-bit Windows has no prebuilt pscat.
$Arch = $env:PROCESSOR_ARCHITEW6432
if (-not $Arch) { $Arch = $env:PROCESSOR_ARCHITECTURE }
$Triple = ''
if ($Arch -eq 'AMD64' -or $Arch -eq 'ARM64') { $Triple = 'x86_64-pc-windows-msvc' }

if ($env:PSCAT_CACHE_DIR) {
    $Cache = $env:PSCAT_CACHE_DIR
} elseif ($env:LOCALAPPDATA) {
    $Cache = Join-Path $env:LOCALAPPDATA 'pscat\plugin'
} else {
    $Cache = Join-Path ([IO.Path]::GetTempPath()) 'pscat\plugin'
}

# --- version check -------------------------------------------------------------
# VersionOk A B: true if version A (X.Y.Z, any suffix ignored) >= B.
# (PowerShell names are case-insensitive: $have is not $Have's twin, $A is $a.)
function VersionOk([string]$Have, [string]$Want) {
    if (-not $Want) { return $true }
    $hv = @(($Have -replace '[-+].*$', '').Split('.') | ForEach-Object { [int64]("0" + ($_ -replace '[^0-9]', '')) })
    $wv = @(($Want -replace '[-+].*$', '').Split('.') | ForEach-Object { [int64]("0" + ($_ -replace '[^0-9]', '')) })
    for ($i = 0; $i -lt 3; $i++) {
        $x = 0; if ($i -lt $hv.Count) { $x = $hv[$i] }
        $y = 0; if ($i -lt $wv.Count) { $y = $wv[$i] }
        if ($x -gt $y) { return $true }
        if ($x -lt $y) { return $false }
    }
    return $true
}

# The version a pscat-mcp reports, or '' if it does not say. Its stdin is
# closed at once, so an old pscat-mcp without --version, which would start
# serving instead, sees EOF and exits.
function Get-McpVersion([string]$Exe) {
    try {
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $Exe
        $psi.Arguments = '--version'
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.RedirectStandardInput = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $p = [Diagnostics.Process]::Start($psi)
        $p.StandardInput.Close()
        # Both streams read asynchronously, so a candidate that hangs with
        # stdout open still meets the deadline.
        $err = $p.StandardError.ReadToEndAsync()
        $outTask = $p.StandardOutput.ReadToEndAsync()
        if (-not $p.WaitForExit(30000)) { try { $p.Kill() } catch { }; return '' }
        $out = $outTask.Result
        $null = $err.Result
        if ($out -match '(?m)^pscat-mcp ([0-9][0-9.]*[^\s]*)') { return $Matches[1] }
    } catch { }
    return ''
}

$script:FoundDir = ''
$script:FoundBin = ''
$script:FoundHow = ''
$script:OldSeen = ''
# TryCandidate PATH HOW: accept PATH (a pscat-mcp) if it runs and is new enough.
$script:Unusable = @()
function TryCandidate([string]$Path, [string]$How) {
    if ($script:FoundDir) { return }
    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) { return }
    if ($script:Unusable -contains $Path) { return }
    $v = Get-McpVersion $Path
    if (-not $v -or -not (VersionOk $v $MinVersion)) {
        $shown = $v; if (-not $shown) { $shown = 'unknown version' }
        $script:OldSeen = "$Path ($shown)"
        return
    }
    $script:FoundBin = $Path # as given: PSCAT_MCP_BIN may name pscat-mcp-0.6.3.exe, say
    $script:FoundDir = Split-Path -Parent ([IO.Path]::GetFullPath($Path))
    $script:FoundHow = $How
}

function CachedDir {
    if (-not $Tag -or -not $Triple) { return '' }
    $d = Join-Path (Join-Path $Cache $Tag) $Triple
    if (Test-Path -LiteralPath (Join-Path $d 'pscat-mcp.exe') -PathType Leaf) { return $d }
    return ''
}

# Lookup: set FoundDir to a directory holding a usable pscat-mcp (and pscat).
function Lookup {
    $script:FoundDir = ''
    if ($env:PSCAT_MCP_BIN) { TryCandidate $env:PSCAT_MCP_BIN 'PSCAT_MCP_BIN' }
    $onPath = @(Get-Command pscat-mcp -CommandType Application -ErrorAction SilentlyContinue)
    if ($onPath.Count -gt 0) { TryCandidate $onPath[0].Source 'PATH' }
    $cargoHome = $env:CARGO_HOME
    if (-not $cargoHome -and $env:USERPROFILE) { $cargoHome = Join-Path $env:USERPROFILE '.cargo' }
    if ($cargoHome) {
        $d = Join-Path $cargoHome 'bin'
        TryCandidate (Join-Path $d 'pscat-mcp.exe') $d
    }
    if (-not $script:FoundDir) {
        $d = CachedDir
        if ($d) {
            $script:FoundDir = $d
            $script:FoundBin = Join-Path $d 'pscat-mcp.exe'
            $script:FoundHow = 'cache'
        }
    }
    return [bool]$script:FoundDir
}

# --- download and install ------------------------------------------------------
function OfflineHelp {
    Say "install pscat yourself instead (cargo install pscat, or the Windows .zip from $RepoUrl/releases put on PATH), then restart your agent"
}

# Fetch URL FILE: '' on success, 'missing' when the server answered with an
# error (or a file:// URL names no file), 'offline' otherwise.
function Fetch([string]$Url, [string]$File) {
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch { }
    $last = 'offline'
    for ($try = 0; $try -lt 3; $try++) {
        $wc = New-Object System.Net.WebClient
        try {
            if ($wc.Proxy) { $wc.Proxy.Credentials = [Net.CredentialCache]::DefaultNetworkCredentials }
            $wc.DownloadFile($Url, $File)
            return ''
        } catch {
            $e = $_.Exception
            while ($e -and -not ($e -is [Net.WebException]) -and $e.InnerException) { $e = $e.InnerException }
            if ($e -is [Net.WebException] -and $e.Status -eq [Net.WebExceptionStatus]::ProtocolError) { return 'missing' }
            $inner = $e
            while ($inner) {
                if ($inner -is [IO.FileNotFoundException] -or $inner -is [IO.DirectoryNotFoundException]) { return 'missing' }
                $inner = $inner.InnerException
            }
            $last = 'offline'
        } finally {
            $wc.Dispose()
        }
        if ($try -lt 2) { Start-Sleep -Seconds 1 }
    }
    return $last
}

# Lower-case hex SHA-256 of a file. .NET, not Get-FileHash: that one lives in
# a script module that Windows PowerShell may fail to load when PSModulePath
# names PowerShell 7's modules (a host started from pwsh, say).
function Get-Sha256([string]$File) {
    $sha = [Security.Cryptography.SHA256]::Create()
    $fs = [IO.File]::OpenRead($File)
    try { $hash = $sha.ComputeHash($fs) } finally { $fs.Dispose(); $sha.Dispose() }
    return (($hash | ForEach-Object { $_.ToString('x2') }) -join '')
}

# Only one process downloads at a time. The lock is an optimisation: the
# publish step below is safe without it. Windows drops the lock (and deletes
# the file) when the process holding it exits, so it can never go stale.
function TakeLock {
    $lockPath = Join-Path $Cache ".lock-$Tag-$Triple"
    $waited = 0
    while ($true) {
        try {
            return New-Object System.IO.FileStream($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite,
                [IO.FileShare]::None, 1, [IO.FileOptions]::DeleteOnClose)
        } catch [System.IO.IOException], [System.UnauthorizedAccessException] {
            # Held by another installer, or (after an antivirus scan kept a
            # handle) still pending deletion: wait. Access denied for half a
            # minute is a cache we may not write: give up sooner.
            if (CachedDir) { return $null }
            if ($_.Exception -is [System.UnauthorizedAccessException] -and $waited -ge 30) {
                Say "cannot create the download lock $lockPath (access denied)"; throw
            }
            if ($waited -eq 0) { Say 'another pscat download is running; waiting for it' }
            $waited++
            if ($waited -ge 600) { Say "gave up waiting for the download lock $lockPath"; throw }
            Start-Sleep -Seconds 1
        }
    }
}

function Install {
    if (-not $Triple) { Say "there is no prebuilt pscat for Windows on $Arch"; OfflineHelp; return $false }
    if (-not $Tag) { Say "no release pinned in $pinFile"; OfflineHelp; return $false }
    $tagDir = Join-Path $Cache $Tag
    try { $null = New-Item -ItemType Directory -Force -Path $tagDir } catch { Say "cannot create the cache directory $tagDir"; return $false }
    try { $lock = TakeLock } catch { return $false }
    $work = ''
    try {
        if (CachedDir) { return $true }
        # Work dirs left by a killed download (an hour old).
        Get-ChildItem -LiteralPath $tagDir -Directory -Filter '.dl.*' -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.LastWriteTime -lt (Get-Date).AddMinutes(-60) } |
            ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }

        $work = Join-Path $tagDir ('.dl.' + [IO.Path]::GetRandomFileName())
        $null = New-Item -ItemType Directory -Path $work
        $sumsUrl = "$BaseUrl/$Tag/SHA256SUMS"
        $r = Fetch $sumsUrl (Join-Path $work 'SHA256SUMS')
        if ($r) {
            Say "could not download $sumsUrl"
            if ($r -eq 'missing') {
                # The server answered (HTTP error) or the file is missing: not offline.
                Say "release $Tag has no SHA256SUMS there: is the release published (not a draft), and is the tag in $pinFile right?"
            } else {
                Say 'pscat is not installed yet and this computer seems to be offline (or GitHub is unreachable); connect and restart your agent'
            }
            OfflineHelp
            return $false
        }
        $suffix = "-$Triple.zip"
        $want = ''
        $asset = ''
        foreach ($line in [IO.File]::ReadAllLines((Join-Path $work 'SHA256SUMS'))) {
            $f = @($line.Trim() -split '\s+')
            if ($f.Count -lt 2) { continue }
            $n = $f[1].TrimStart('*')
            if ($n.StartsWith('pscat-') -and $n.Length -gt $suffix.Length -and $n.EndsWith($suffix)) {
                $want = $f[0].ToLowerInvariant(); $asset = $n; break
            }
        }
        if (-not $asset -or $asset -notmatch '^[A-Za-z0-9._-]+$') { Say "release $Tag has no archive for $Triple"; OfflineHelp; return $false }
        $url = "$BaseUrl/$Tag/$asset"
        Say "downloading $asset from $RepoUrl ($Tag); this happens once"
        $zip = Join-Path $work $asset
        if (Fetch $url $zip) {
            Say "could not download $url (offline, or GitHub is unreachable); connect and restart your agent"
            OfflineHelp
            return $false
        }
        $got = Get-Sha256 $zip
        if ($got -ne $want) {
            Say "checksum mismatch for ${asset}: SHA256SUMS says $want, the download is $got; not installing it"
            return $false
        }
        $x = Join-Path $work 'x'
        try {
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            [IO.Compression.ZipFile]::ExtractToDirectory($zip, $x)
        } catch { Say "could not unpack ${asset}: $($_.Exception.Message)"; return $false }
        Remove-Item -LiteralPath $zip -Force
        $name = $asset.Substring(0, $asset.Length - 4)
        $bundle = Join-Path $x $name
        if (-not (Test-Path -LiteralPath (Join-Path $bundle 'pscat-mcp.exe') -PathType Leaf)) {
            Say "$asset holds no $name\pscat-mcp.exe"
            return $false
        }

        # Publish: one directory rename puts the whole bundle at $Tag\$Triple.
        # A rename onto an existing name fails, so two racing installers cannot
        # both win, and a reader sees either nothing or a whole bundle. An
        # antivirus scan of the new .exe can hold the directory for a moment,
        # so a failed rename with no winner is retried.
        $dest = Join-Path $tagDir $Triple
        for ($i = 0; $i -lt 20; $i++) {
            try { [IO.Directory]::Move($bundle, $dest); Say "installed pscat $Tag in $dest"; break }
            catch {
                if (Test-Path -LiteralPath $dest) { break } # another installer won
                Start-Sleep -Milliseconds 500
            }
        }
        return [bool](CachedDir)
    } finally {
        if ($work) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
        if ($lock) { $lock.Dispose() }
    }
}

# Ensure: lookup, else install, else (offline) any earlier cached version.
function Ensure {
    if (Lookup) { return $true }
    if ($script:OldSeen) { Say "ignoring $($script:OldSeen): this plugin needs pscat $MinVersion or newer" }
    if ((Install) -and (Lookup)) { return $true }
    if ($Triple -and (Test-Path -LiteralPath $Cache)) {
        $prev = @(Get-ChildItem -LiteralPath $Cache -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path (Join-Path $_.FullName $Triple) 'pscat-mcp.exe' } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
            Sort-Object { (Get-Item -LiteralPath $_).LastWriteTime } -Descending)
        if ($prev.Count -gt 0) {
            $script:FoundBin = $prev[0]
            $script:FoundDir = Split-Path -Parent $prev[0]
            $script:FoundHow = 'cache'
            Say "using the previously downloaded pscat in $($script:FoundDir) instead"
            return $true
        }
    }
    return $false
}

# --- the --exe handoff -------------------------------------------------------------
# How --exe tells pscat-mcp.cmd which program to run: VAR|REST for %VAR%\REST,
# or -|PATH. cmd's `for /f` decodes it with the console code page, which
# cannot spell every path (a user name with accented or non-Latin letters,
# say). So the user-specific start of a path travels as the name of an
# environment variable the caller already holds in Unicode, and only an
# ASCII rest as text. Any other path goes as itself if the code page can
# spell it, else as its 8.3 short name (ASCII). '' if neither works.
function ExeHandoff([string]$Path) {
    foreach ($var in 'PSCAT_CACHE_DIR', 'LOCALAPPDATA', 'CARGO_HOME', 'USERPROFILE') {
        $base = [Environment]::GetEnvironmentVariable($var)
        if (-not $base) { continue }
        $base = $base.TrimEnd('\', '/')
        if ($Path.Length -gt $base.Length + 1 -and
            $Path.StartsWith($base, [StringComparison]::OrdinalIgnoreCase) -and
            ($Path[$base.Length] -eq '\' -or $Path[$base.Length] -eq '/')) {
            $rest = $Path.Substring($base.Length + 1)
            # Plain ASCII with nothing cmd would expand or split on.
            if ($rest -match '^[A-Za-z0-9 ._\\/()+,=@#~-]+$') { return "$var|$rest" }
        }
    }
    $enc = [Console]::OutputEncoding
    if ($enc.GetString($enc.GetBytes($Path)) -eq $Path) { return "-|$Path" }
    try {
        $short = (New-Object -ComObject Scripting.FileSystemObject).GetFile($Path).ShortPath
        if ($short -and $enc.GetString($enc.GetBytes($short)) -eq $short) { return "-|$short" }
    } catch { }
    return ''
}

# --- SessionStart hook -----------------------------------------------------------
function Json-Str([string]$S) {
    $sb = New-Object System.Text.StringBuilder
    $null = $sb.Append('"')
    foreach ($c in $S.ToCharArray()) {
        switch ($c) {
            '"' { $null = $sb.Append('\"') }
            '\' { $null = $sb.Append('\\') }
            default {
                if ([int]$c -lt 0x20) { $null = $sb.Append(' ') } else { $null = $sb.Append($c) }
            }
        }
    }
    $null = $sb.Append('"')
    return $sb.ToString()
}

function Hook {
    if (Lookup) {
        if ($script:FoundHow -eq 'PATH' -and @(Get-Command pscat -CommandType Application -ErrorAction SilentlyContinue).Count -gt 0) {
            exit 0 # the usual case: nothing to say
        }
        $ctx = "pscat plugin: the pscat command-line tool is not on PATH in this session. Run it by its full path, $(Join-Path $script:FoundDir 'pscat.exe') (pscat-mcp is beside it)."
        Out-Utf8 ('{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": ' + (Json-Str $ctx) + '}}')
        exit 0
    }
    # Single quotes: the same command works in Git Bash and in PowerShell.
    $launcher = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File '$(Join-Path $Here 'pscat-mcp.ps1')' --locate"
    if (-not $Triple) {
        $msg = "pscat plugin: pscat / pscat-mcp not found, and there is no prebuilt pscat for Windows on $Arch, so the pscat MCP tools will not start. Install them with: cargo install pscat (or a release from $RepoUrl/releases), then restart your agent."
        $ctx = $msg
    } else {
        $t = $Tag; if (-not $t) { $t = '?' }
        $msg = "pscat plugin: pscat is not installed yet. The pscat MCP server downloads it (release $t, about 6 MB, checked against the release's SHA256SUMS) from $RepoUrl the first time it starts, into $Cache. If the pscat tools do not appear, check the internet connection and restart; or install pscat yourself: cargo install pscat."
        if ($script:OldSeen) { $msg = "$msg (Found $($script:OldSeen), older than the $MinVersion this plugin needs.)" }
        $ctx = "pscat plugin: pscat is not on PATH. To use the pscat command line, run: $launcher  (it downloads pscat if needed and prints the directory holding pscat.exe and pscat-mcp.exe), then run that directory's pscat.exe by its full path."
    }
    Out-Utf8 ('{"systemMessage": ' + (Json-Str $msg) + ', "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": ' + (Json-Str $ctx) + '}}')
    exit 0
}

$mode = ''
if ($args.Count -gt 0) { $mode = [string]$args[0] }
switch ($mode) {
    '--hook' { Hook }
    '--locate' {
        if (-not (Ensure)) { exit 1 }
        Out-Utf8 $script:FoundDir
        exit 0
    }
    '--exe-utf8' {
        # For the sh entry point under Git Bash, which reads UTF-8: the path
        # itself, losslessly.
        if (-not (Ensure)) { Say 'pscat-mcp is not available, so the pscat tools cannot start'; exit 1 }
        if ($script:FoundHow -ne 'cache' -and $script:FoundHow -ne 'PATH') { Say "using $($script:FoundBin)" }
        Out-Utf8 $script:FoundBin
        exit 0
    }
    '--exe' {
        # For pscat-mcp.cmd; see ExeHandoff. An installed copy whose path cannot be handed over is
        # passed over for the next candidate (in the end the cache, whose
        # path always can be).
        while ($true) {
            if (-not (Ensure)) { Say 'pscat-mcp is not available, so the pscat tools cannot start'; exit 1 }
            $h = ExeHandoff $script:FoundBin
            if ($h) { break }
            if ($script:FoundHow -eq 'cache') {
                Say "cmd.exe cannot be given the path $($script:FoundBin); set PSCAT_CACHE_DIR to a folder whose path has only ASCII letters"
                exit 1
            }
            Say "passing over $($script:FoundBin): cmd.exe cannot be given its path (letters outside the console code page, and no 8.3 short name)"
            $script:Unusable += $script:FoundBin
        }
        if ($script:FoundHow -ne 'cache' -and $script:FoundHow -ne 'PATH') { Say "using $($script:FoundBin)" }
        [Console]::Out.WriteLine($h)
        exit 0
    }
}

if (-not (Ensure)) {
    Say 'pscat-mcp is not available, so the pscat tools cannot start'
    exit 1
}
if ($script:FoundHow -ne 'cache' -and $script:FoundHow -ne 'PATH') { Say "using $($script:FoundBin)" }
Remove-Item Env:PSCAT_PLUGIN_LAUNCHER
& $script:FoundBin @args
exit $LASTEXITCODE
