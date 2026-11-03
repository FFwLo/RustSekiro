# FCC worker runner: takes tasks from fcc/queue one at a time and runs each with a free-model
# `fcc-claude -p` worker (Free Claude Code proxy, 127.0.0.1:8082). Opus sessions write the tasks
# and review the results; workers never commit. See fcc/README.md for the task format.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\fcc-worker.ps1          # run until the queue is empty
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\fcc-worker.ps1 -Once    # run one task
#   ... -Task bosses-T1-003                                                            # run that task only
#
# Every task runs in a snapshot of the current tree (tracked + untracked files, so other
# sessions' uncommitted edits are included) at ..\fcc-work\<id>, with its own
# git repo and a "base" commit. The worker's edits come back as fcc/done/<id>.patch against
# that base; tools\fcc-review.ps1 accept applies it to the real tree.

param(
    [switch] $Once,
    [string] $Task = ""
)

$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
$Fcc = Join-Path $Repo "fcc"
$WorkRoot = Join-Path (Resolve-Path "$PSScriptRoot\..\..") "fcc-work"
# The workspace folder in the //c/... form of the permission rules.
$WsPath = (Resolve-Path "$PSScriptRoot\..\..").Path
$Ws = '//' + $WsPath.Substring(0, 1).ToLower() + ($WsPath.Substring(2) -replace '\\', '/')
$TargetDir = Join-Path $Repo "target\fcc"
foreach ($d in "queue", "running", "done", "accepted", "rejected") { New-Item -ItemType Directory -Force (Join-Path $Fcc $d) | Out-Null }
New-Item -ItemType Directory -Force $WorkRoot | Out-Null

$env:Path = [Environment]::GetEnvironmentVariable("Path", "User") + ";" + [Environment]::GetEnvironmentVariable("Path", "Machine")
$FccClaude = (Get-Command fcc-claude -ErrorAction Stop).Source
# Tests in the snapshot read extracted/ and config.toml from the real project folder.
$env:SHINOBI_ROOT = $Repo
$env:CARGO_TARGET_DIR = $TargetDir

# Only one runner at a time (builds would pile up otherwise).
$LockPath = Join-Path $Fcc "runner.lock"
try { $Lock = [IO.File]::Open($LockPath, "OpenOrCreate", "ReadWrite", "None") }
catch { Write-Host "Another fcc-worker is running ($LockPath)."; exit 0 }

# Tier settings: tools the worker gets and its turn budget.
$ReadTools = @("Read", "Grep", "Glob", "Bash(cargo test:*)", "Bash(cargo check:*)", "Bash(cargo clippy:*)", "Bash(cargo build:*)")
$Tiers = @{
    "T0" = @{ Tools = $ReadTools; Turns = 12; Edits = $false }
    "T1" = @{ Tools = $ReadTools + @("Edit", "Write"); Turns = 40; Edits = $true }
    "T2" = @{ Tools = $ReadTools + @("Edit", "Write"); Turns = 60; Edits = $true }
}
$Denied = @(
    "NotebookEdit", "WebFetch", "WebSearch", "Agent",
    "Read($Ws/extracted/**)", "Read($Ws/shinobi-combat/extracted/**)",
    "Read($Ws/reference/**)", "Read($Ws/sekiro-rs/**)",
    "Read($Ws/shinobi-combat/tools/DSLuaDecompiler/**)"
)
# Never part of an accepted patch, whatever the task says.
$NeverTouch = @("config.toml", "Cargo.lock", ".cargo/*", ".github/*")

function Read-TaskFile([string] $Path) {
    $lines = Get-Content -LiteralPath $Path -Encoding UTF8
    $meta = @{}
    $i = 0
    for (; $i -lt $lines.Count; $i++) {
        if ($lines[$i].Trim() -eq "---") { $i++; break }
        if ($lines[$i] -match '^\s*([a-z_]+)\s*:\s*(.*)$') { $meta[$Matches[1]] = $Matches[2].Trim() }
    }
    $meta["body"] = ($lines[$i..($lines.Count - 1)] -join "`n").Trim()
    $meta["path"] = $Path
    if (-not $meta["id"]) { $meta["id"] = [IO.Path]::GetFileNameWithoutExtension($Path) }
    if (-not $meta["tier"]) { $meta["tier"] = "T0" }
    if (-not $Tiers.ContainsKey($meta["tier"])) { throw "Task $($meta.id): unknown tier '$($meta.tier)'" }
    $meta["files"] = @(($meta["files"] -split ",") | ForEach-Object { $_.Trim().Replace("\", "/") } | Where-Object { $_ })
    return $meta
}

function Invoke-Cmd([string] $CommandLine, [string] $WorkDir, [string] $OutFile, [string] $InFile = "") {
    $redir = if ($InFile) { " < `"$InFile`"" } else { "" }
    Push-Location $WorkDir
    try {
        cmd /c "$CommandLine$redir > `"$OutFile`" 2>&1"
        return $LASTEXITCODE
    }
    finally { Pop-Location }
}

function New-Snapshot([string] $Id) {
    $dest = Join-Path $WorkRoot $Id
    if (Test-Path $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
    New-Item -ItemType Directory -Force $dest | Out-Null
    Push-Location $Repo
    try { $files = git ls-files -co --exclude-standard } finally { Pop-Location }
    foreach ($f in $files) {
        $src = Join-Path $Repo $f
        if (-not (Test-Path -LiteralPath $src -PathType Leaf)) { continue }
        $dst = Join-Path $dest $f
        $dir = Split-Path -Parent $dst
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
        Copy-Item -LiteralPath $src -Destination $dst
    }
    Push-Location $dest
    try {
        git init -q
        git config core.autocrlf false
        git add -A
        git -c user.name=fcc -c user.email=fcc@localhost commit -q -m base
    }
    finally { Pop-Location }
    return $dest
}

function Test-InScope([string] $File, [string[]] $Allowed) {
    foreach ($n in $NeverTouch) { if ($File -like $n) { return $false } }
    if ($Allowed.Count -eq 0) { return $false }
    foreach ($a in $Allowed) { if ($File -like $a) { return $true } }
    return $false
}

function Get-TestSummary([string] $Text) {
    $lines = $Text -split "`r?`n"
    $keep = $lines | Where-Object { $_ -match 'test result:|FAILED|panicked at|^error(\[|:)|^warning: unused|^\s+--> ' } | Select-Object -First 60
    $tail = $lines | Select-Object -Last 15
    return (($keep + "..." + $tail) -join "`n")
}

function Invoke-Task($T) {
    $id = $T.id
    $tier = $Tiers[$T.tier]
    $running = Join-Path $Fcc "running\$id.md"
    Move-Item -LiteralPath $T.path -Destination $running -Force
    $log = Join-Path $Fcc "running\$id.log"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    Write-Host "==> $id ($($T.tier), owner $($T.owner)): snapshot"
    $work = New-Snapshot $id

    $editRule = if ($tier.Edits) {
        "You MAY edit only these files (paths relative to the current folder): $($T.files -join ', '). Change nothing else. Keep the surrounding code style and comment density."
    } else {
        "You must NOT edit, create or delete any file."
    }
    $prompt = @"
You are an FCC worker on the Rust project in the current folder (Sv1, a Sekiro combat clone). Rules:
- $editRule
- Do not run git. Do not read anything under ..\extracted, ..\reference, ..\sekiro-rs or any decomp folder.
- Never invent game values (frame counts, damage, speeds, ids). If the task needs a value it does not give you, stop and say so in NOTES.
- Build and test only with: cargo test --profile quick --lib <filter>  (or cargo check / cargo clippy). CARGO_TARGET_DIR is already set; do not pass --target-dir. Never run the full test suite unless the task says so.

TASK $id
$($T.body)

When done, reply with exactly this format and nothing else:
STATUS: <done | blocked>
CHANGED: <comma-separated files, or none>
RESULT: <one or two lines: what you did or found>
NOTES: <anything the reviewer must know, or none>
"@
    $promptFile = Join-Path $work ".fcc-prompt.txt"
    [IO.File]::WriteAllText($promptFile, $prompt, [Text.UTF8Encoding]::new($false))

    $toolArgs = ($tier.Tools | ForEach-Object { "`"$_`"" }) -join " "
    $denyArgs = ($Denied | ForEach-Object { "`"$_`"" }) -join " "
    $turns = if ($T.max_turns) { $T.max_turns } else { $tier.Turns }
    $modelArg = if ($T.model) { " --model `"$($T.model)`"" } else { "" }
    $cmdLine = "`"$FccClaude`" -p --allowedTools $toolArgs --disallowedTools $denyArgs --max-turns $turns$modelArg"
    Write-Host "==> $id`: worker"
    $workerOut = Join-Path $work ".fcc-worker.txt"
    $workerExit = Invoke-Cmd $cmdLine $work $workerOut $promptFile
    $report = (Get-Content -LiteralPath $workerOut -Raw -Encoding UTF8)
    if ($null -eq $report) { $report = "" }

    # The runner's own check: never trust the worker's word for the test result.
    $verifyText = "(no verify command)"
    $verifyExit = $null
    if ($T.verify) {
        Write-Host "==> $id`: verify"
        $verifyOut = Join-Path $work ".fcc-verify.txt"
        $verifyExit = Invoke-Cmd $T.verify $work $verifyOut
        $verifyText = Get-TestSummary (Get-Content -LiteralPath $verifyOut -Raw -Encoding UTF8)
    }

    # Diff against the base snapshot.
    Remove-Item -LiteralPath $promptFile, $workerOut -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $work ".fcc-verify.txt") -ErrorAction SilentlyContinue
    Push-Location $work
    try {
        git add -A
        $changed = @(git diff --cached --name-only HEAD)
        $stat = (git diff --cached --stat HEAD) -join "`n"
        $patchPath = Join-Path $Fcc "done\$id.patch"
        if ($changed.Count -gt 0) {
            $patch = (git diff --cached --binary HEAD) -join "`n"
            [IO.File]::WriteAllText($patchPath, $patch + "`n", [Text.UTF8Encoding]::new($false))
        }
    }
    finally { Pop-Location }
    $outOfScope = @($changed | Where-Object { -not (Test-InScope $_ $T.files) })
    if (-not $tier.Edits -and $changed.Count -gt 0) { $outOfScope = $changed }

    $status = "REVIEW"
    if ($workerExit -ne 0) { $status = "WORKER-ERROR" }
    elseif ($outOfScope.Count -gt 0) { $status = "OUT-OF-SCOPE" }
    elseif ($null -ne $verifyExit -and $verifyExit -ne 0) { $status = "VERIFY-FAILED" }
    elseif ($report -match 'STATUS:\s*blocked') { $status = "BLOCKED" }

    $result = @"
# $id - $status

- tier: $($T.tier) / owner: $($T.owner) / elapsed: $([int]$sw.Elapsed.TotalSeconds) s / worker exit: $workerExit / verify exit: $verifyExit
- snapshot: $work
- patch: $(if ($changed.Count -gt 0) { "fcc/done/$id.patch" } else { "none" })
- out of scope: $(if ($outOfScope.Count -gt 0) { $outOfScope -join ', ' } else { "none" })

## Worker report
``````
$($report.Trim())
``````

## Verify (run by the runner: $($T.verify))
``````
$verifyText
``````

## Diffstat
``````
$stat
``````
"@
    [IO.File]::WriteAllText((Join-Path $Fcc "done\$id.result.md"), $result, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $running -Destination (Join-Path $Fcc "done\$id.md") -Force
    Remove-Item -LiteralPath $log -ErrorAction SilentlyContinue
    if ($changed.Count -eq 0) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Host "==> $id`: $status ($([int]$sw.Elapsed.TotalSeconds) s)"
}

try {
    while ($true) {
        $next = if ($Task) { Get-Item (Join-Path $Fcc "queue\$Task.md") -ErrorAction SilentlyContinue }
                else { Get-ChildItem (Join-Path $Fcc "queue") -Filter *.md | Sort-Object LastWriteTime | Select-Object -First 1 }
        if (-not $next) { Write-Host "Queue empty."; break }
        $t = Read-TaskFile $next.FullName
        try { Invoke-Task $t }
        catch {
            Write-Host "==> $($t.id): RUNNER-ERROR $($_.Exception.Message)"
            [IO.File]::WriteAllText((Join-Path $Fcc "done\$($t.id).result.md"), "# $($t.id) - RUNNER-ERROR`n`n$($_ | Out-String)", [Text.UTF8Encoding]::new($false))
            $r = Join-Path $Fcc "running\$($t.id).md"
            if (Test-Path $r) { Move-Item -LiteralPath $r -Destination (Join-Path $Fcc "done\$($t.id).md") -Force }
        }
        if ($Once -or $Task) { break }
    }
}
finally { $Lock.Dispose() }
