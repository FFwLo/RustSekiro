# Review FCC worker results (see tools\fcc-worker.ps1 and fcc/README.md). Only Opus sessions
# accept or reject, and only for their own tasks (the id prefix is the owner).
#
#   tools\fcc-review.ps1 list                    # results waiting for review
#   tools\fcc-review.ps1 show   <id>             # result + patch
#   tools\fcc-review.ps1 accept <id>             # apply the patch to the real tree (no commit)
#   tools\fcc-review.ps1 reject <id> "<reason>"  # file it under fcc/rejected with the reason

param(
    [Parameter(Mandatory = $true, Position = 0)][ValidateSet("list", "show", "accept", "reject")] [string] $Action,
    [Parameter(Position = 1)] [string] $Id = "",
    [Parameter(Position = 2)] [string] $Reason = ""
)

$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
$Fcc = Join-Path $Repo "fcc"
$WorkRoot = "C:\DeadlockModding\fcc-work"

function Move-Results([string] $To, [string] $Note) {
    foreach ($ext in ".md", ".result.md", ".patch") {
        $f = Join-Path $Fcc "done\$Id$ext"
        if (Test-Path $f) { Move-Item -LiteralPath $f -Destination (Join-Path $Fcc "$To\$Id$ext") -Force }
    }
    if ($Note) { Add-Content -LiteralPath (Join-Path $Fcc "$To\$Id.result.md") -Value "`n## Review`n$Note" -Encoding UTF8 }
    $w = Join-Path $WorkRoot $Id
    if (Test-Path $w) { Remove-Item -LiteralPath $w -Recurse -Force }
}

switch ($Action) {
    "list" {
        Get-ChildItem (Join-Path $Fcc "done") -Filter *.result.md -ErrorAction SilentlyContinue | ForEach-Object {
            (Get-Content -LiteralPath $_.FullName -TotalCount 1).TrimStart("# ")
        }
        $q = @(Get-ChildItem (Join-Path $Fcc "queue") -Filter *.md -ErrorAction SilentlyContinue).Count
        $r = @(Get-ChildItem (Join-Path $Fcc "running") -Filter *.md -ErrorAction SilentlyContinue).Count
        Write-Host "queued: $q, running: $r"
    }
    "show" {
        Get-Content -LiteralPath (Join-Path $Fcc "done\$Id.result.md") -Encoding UTF8
        $p = Join-Path $Fcc "done\$Id.patch"
        if (Test-Path $p) { Write-Host "`n## Patch"; Get-Content -LiteralPath $p -Encoding UTF8 }
    }
    "accept" {
        $p = Join-Path $Fcc "done\$Id.patch"
        if (Test-Path $p) {
            Push-Location $Repo
            try {
                git apply --check --whitespace=nowarn $p
                if ($LASTEXITCODE -ne 0) { throw "Patch no longer applies (the real tree changed since the snapshot). Redo the change by hand or requeue the task." }
                git apply --whitespace=nowarn $p
                if ($LASTEXITCODE -ne 0) { throw "git apply failed." }
            }
            finally { Pop-Location }
            Write-Host "Applied $Id (not committed)."
        }
        Move-Results "accepted" "accepted $(Get-Date -Format s)"
    }
    "reject" {
        if (-not $Reason) { throw "reject needs a reason." }
        Move-Results "rejected" "rejected $(Get-Date -Format s): $Reason"
        Write-Host "Rejected $Id."
    }
}
