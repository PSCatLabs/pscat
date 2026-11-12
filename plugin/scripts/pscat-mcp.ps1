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
#   pscat-mcp.ps1 --exe [args]       find or install, tell pscat-mcp.cmd the
#                                    path of pscat-mcp and the output root to
#                                    pass it (see ExeHandoff, OutRoot)
#   pscat-mcp.ps1 --exe-utf8 [args]  the same as plain UTF-8 (for Git Bash)
#   pscat-mcp.ps1 [pscat-mcp args]   find or install, then run pscat-mcp
#
# The launcher-only --plugin-out-root=DIR and --plugin-project-dir=DIR
# arguments (#910) are taken out before pscat-mcp sees the arguments; the
# output root is chosen as pscat-mcp.sh chooses it (see "The output root").
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
    foreach ($var in 'PSCAT_CACHE_DIR', 'LOCALAPPDATA', 'APPDATA', 'CARGO_HOME', 'USERPROFILE') {
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

# --- The output root (issue #910) ------------------------------------------------
# Where the server's out_path may write: one directory, chosen here at
# registration, never by a tool call, exactly as pscat-mcp.sh chooses it,
# unless the arguments already hold --out-root. The first usable of:
#   1. --plugin-out-root=DIR: Claude Code's `output_dir` plugin option;
#   2. `out-root = DIR` in %APPDATA%\pscat\plugin.conf ($env:XDG_CONFIG_HOME\
#      pscat\plugin.conf when that is set): every host, and the only way
#      under Codex, whose plugin servers take no user arguments;
#   3. --plugin-project-dir=DIR: the project Claude Code names;
#   4. the working directory, which the server takes by default (so nothing
#      is passed);
#   5. renders\ in pscat's per-user data directory (%APPDATA%\pscat\renders),
#      created if need be.
# Usable: an absolute path (or ~\...) to an existing directory that is not a
# drive root and not inside this plugin. The launcher-only --plugin-*
# arguments are not passed on.
$ConfFile = ''
if ($env:XDG_CONFIG_HOME) { $ConfFile = Join-Path $env:XDG_CONFIG_HOME 'pscat\plugin.conf' }
elseif ($env:APPDATA) { $ConfFile = Join-Path $env:APPDATA 'pscat\plugin.conf' }

$script:PluginOutRoot = ''
$script:PluginProjectDir = ''
$script:HaveOutRoot = $false
$script:Rest = @()      # the arguments for pscat-mcp: all but the --plugin-* ones
$script:Dropped = 0     # how many --plugin-* arguments there were
# SplitArgs ARGS: sort the launcher-only --plugin-* arguments from the rest.
function SplitArgs([object[]]$List) {
    $script:Rest = @()
    foreach ($a in $List) {
        $a = [string]$a
        if ($a -match '^--plugin-out-root=(.*)$') { $script:PluginOutRoot = $Matches[1]; $script:Dropped++ }
        elseif ($a -match '^--plugin-project-dir=(.*)$') { $script:PluginProjectDir = $Matches[1]; $script:Dropped++ }
        else {
            if ($a -eq '--out-root') { $script:HaveOutRoot = $true }
            $script:Rest += $a
        }
    }
}

# The arguments the launcher was given. pscat-mcp.cmd and the sh entry point
# put them, quoted as on a command line, in PSCAT_LAUNCH_ARGS instead of on
# PowerShell's own command line, whose parsing of a bare `--name=` or `--name=C:\`
# argument is not ours to rely on. Without that variable, the arguments after
# the mode are used.
function ModeArgs([object[]]$Given) {
    if ($null -ne $env:PSCAT_LAUNCH_ARGS) {
        $list = New-Object System.Collections.ArrayList
        foreach ($m in [regex]::Matches($env:PSCAT_LAUNCH_ARGS, '(?:[^\s"]+|"[^"]*")+')) {
            $null = $list.Add($m.Value.Replace('"', ''))
        }
        return $list.ToArray()
    }
    return @($Given | Select-Object -Skip 1)
}

# UsableRoot VALUE [WHAT]: VALUE as a full directory path if it may be the
# output root, else ''. WHAT names the setting in the warning; without it, quiet.
function UsableRoot([string]$Value, [string]$What) {
    $v = $Value
    if (-not $v -or $v.Contains('${')) { return '' } # unset, or a placeholder the host left unexpanded
    $userHome = $env:USERPROFILE
    if (-not $userHome) { $userHome = $env:HOME }
    if ($v -eq '~') { $v = $userHome }
    elseif ($v.StartsWith('~\') -or $v.StartsWith('~/')) {
        if ($userHome) { $v = Join-Path $userHome $v.Substring(2) } else { $v = '' }
    }
    $why = ''
    $d = ''
    if (-not $v -or $v -notmatch '^([A-Za-z]:[\\/]|\\\\)') { $why = 'not an absolute path' }
    elseif (-not (Test-Path -LiteralPath $v -PathType Container)) { $why = 'not an existing directory' }
    else {
        $full = [IO.Path]::GetFullPath($v)
        if ($full -eq [IO.Path]::GetPathRoot($full)) { $why = 'a drive root' }
        else {
            $d = $full.TrimEnd('\', '/')
            $plugin = [IO.Path]::GetFullPath($PluginDir).TrimEnd('\', '/')
            if (($d + '\').StartsWith($plugin + '\', [StringComparison]::OrdinalIgnoreCase)) {
                $why = "inside the plugin's own directory"
            }
        }
    }
    if ($why) {
        if ($What) { Say "ignoring $What ${Value}: $why" }
        return ''
    }
    return $d
}

# The value of the last `out-root = VALUE` line in the config file.
function ConfOutRoot {
    if (-not $ConfFile -or -not (Test-Path -LiteralPath $ConfFile -PathType Leaf)) { return '' }
    $val = ''
    try {
        foreach ($line in [IO.File]::ReadAllLines($ConfFile)) {
            if ($line -match '^\s*out-root\s*=\s*(.*?)\s*$') { $val = $Matches[1] -replace '^"(.*)"$', '$1' }
        }
    } catch { }
    return $val
}

# renders\ in pscat's per-user data directory (src/paths.rs, user_data_dir).
function DefaultRoot {
    if (-not $env:APPDATA) { return '' }
    $d = Join-Path $env:APPDATA 'pscat\renders'
    try { $null = New-Item -ItemType Directory -Path $d -Force -ErrorAction Stop }
    catch { Say "cannot create $d for renders"; return '' }
    return (UsableRoot $d 'the default output directory')
}

# ChooseOutRoot HANDOFF: the output root to pass as --out-root, or '' to leave
# the working directory (or an explicit --out-root) in charge. With HANDOFF,
# the answer is in ExeHandoff's form and a root that form cannot carry is
# passed over for the next candidate; without it, the plain path.
function ChooseOutRoot([bool]$Handoff) {
    if ($script:HaveOutRoot) { return '' }
    $candidates = @(
        @($script:PluginOutRoot, "the plugin's output_dir setting"),
        @((ConfOutRoot), "out-root in $ConfFile"),
        @($script:PluginProjectDir, '')
    )
    foreach ($c in $candidates) {
        $d = UsableRoot $c[0] $c[1]
        if (-not $d) { continue }
        if (-not $Handoff) { return $d }
        $h = ExeHandoff $d
        if ($h) { return $h }
        Say "passing over the output directory ${d}: cmd.exe cannot be given its path (letters outside the console code page, and no 8.3 short name)"
    }
    if (UsableRoot (Get-Location -PSProvider FileSystem).ProviderPath '') { return '' }
    $d = DefaultRoot
    if ($d) {
        if (-not $Handoff) { return $d }
        $h = ExeHandoff $d
        if ($h) { return $h }
        Say "cmd.exe cannot be given the path $d; set APPDATA to a folder whose path has only ASCII letters, or out-root in $ConfFile"
    }
    Say 'no usable output directory, so out_path is off'
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
        $msg = "pscat plugin: pscat is not installed yet. The pscat MCP server downloads it (release $t, about 14 MB, checked against the release's SHA256SUMS) from $RepoUrl the first time it starts, into $Cache. If the pscat tools do not appear, check the internet connection and restart; or install pscat yourself: cargo install pscat."
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
        # For the sh entry point under Git Bash, which reads UTF-8: the exe's
        # path on the first line and the output root to pass (or nothing) on
        # the second, both losslessly.
        SplitArgs @(ModeArgs $args)
        if (-not (Ensure)) { Say 'pscat-mcp is not available, so the pscat tools cannot start'; exit 1 }
        if ($script:FoundHow -ne 'cache' -and $script:FoundHow -ne 'PATH') { Say "using $($script:FoundBin)" }
        Out-Utf8 ($script:FoundBin + "`n" + (ChooseOutRoot $false))
        exit 0
    }
    '--exe' {
        # For pscat-mcp.cmd; see ExeHandoff. An installed copy whose path cannot be handed over is
        # passed over for the next candidate (in the end the cache, whose
        # path always can be). Lines, each `TAG|...`:
        #   E|VAR|REST or E|-|PATH   the program to run
        #   R|VAR|REST or R|-|PATH   the output root, if one is to be passed
        #   A|drop or A|keep         drop the .cmd's own arguments (they were
        #                            all launcher-only) or pass them on
        SplitArgs @(ModeArgs $args)
        if ($script:Dropped -gt 0 -and $script:Rest.Count -gt 0) {
            Say "pscat-mcp.cmd takes the --plugin-* options alone; to give pscat-mcp other arguments ($($script:Rest -join ' ')), register pscat-mcp.exe directly"
            exit 1
        }
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
        $root = ChooseOutRoot $true
        [Console]::Out.WriteLine("E|$h")
        if ($root) { [Console]::Out.WriteLine("R|$root") }
        if ($script:Dropped -gt 0) { [Console]::Out.WriteLine('A|drop') } else { [Console]::Out.WriteLine('A|keep') }
        exit 0
    }
}

SplitArgs $args
if (-not (Ensure)) {
    Say 'pscat-mcp is not available, so the pscat tools cannot start'
    exit 1
}
if ($script:FoundHow -ne 'cache' -and $script:FoundHow -ne 'PATH') { Say "using $($script:FoundBin)" }
$root = ChooseOutRoot $false
$serverArgs = @($script:Rest)
if ($root) { $serverArgs = @('--out-root', $root) + $serverArgs }
Remove-Item Env:PSCAT_PLUGIN_LAUNCHER
& $script:FoundBin @serverArgs
exit $LASTEXITCODE
