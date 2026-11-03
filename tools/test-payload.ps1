# Proof-of-concept / regression test for the two export modes of
# html-to-exe-shell. Builds a test game in a temp dir, produces either
# a folder-mode exe or a single-file exe using the append format from
# docs/format.md, launches it, and verifies the process stays alive.

param(
    [Parameter(Mandatory = $false)]
    [ValidateSet("folder", "embedded")]
    [string]$Mode = "folder"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Path $PSScriptRoot -Parent
$shellExe = Join-Path $repoRoot "shell\target\release\game-shell.exe"
if (-not (Test-Path $shellExe)) {
    throw "shell not built yet: $shellExe"
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("htx-test-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $work -Force | Out-Null
Write-Host "work dir: $work"

# ---------------------------------------------------------------- test game

$game = Join-Path $work "src-game"
New-Item -ItemType Directory -Path $game | Out-Null

Set-Content -Path (Join-Path $game "data.json") -Value '{"hello": "custom protocol works"}'

Set-Content -Path (Join-Path $game "app.js") -Encoding UTF8 @'
fetch("data.json")
    .then(function (r) { return r.json(); })
    .then(function (d) {
        document.getElementById("msg").textContent = "fetch  OK: " + JSON.stringify(d);
    })
    .catch(function (e) {
        document.getElementById("msg").textContent = "fetch FAILED: " + e;
    });

var runs = (parseInt(localStorage.getItem("runs"), 10) || 0) + 1;
localStorage.setItem("runs", String(runs));

var canvas = document.getElementById("c");
var ctx = canvas.getContext("2d");
var t = 0;
setInterval(function () {
    t += 0.05;
    ctx.fillStyle = "#112233";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#e74c3c";
    ctx.beginPath();
    ctx.arc(48 + 30 * Math.cos(t), 48 + 30 * Math.sin(t), 8, 0, Math.PI * 2);
    ctx.fill();
}, 16);
'@

Set-Content -Path (Join-Path $game "index.html") -Encoding UTF8 @'
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <title>Shell PoC</title>
    <style>body { background:#112233; color:#ecf0f1; font:16px sans-serif; }</style>
</head>
<body>
    <h1 id="msg">loading...</h1>
    <canvas id="c" width="96" height="96"></canvas>
    <script src="app.js"></script>
</body>
</html>
'@

New-Item -ItemType Directory -Path (Join-Path $game "assets") | Out-Null
Set-Content -Path (Join-Path $game "assets\nested.txt") -Value "subdirectory payload" # force a nested zip dir

# ---------------------------------------------------------------- config

$config = [IO.Path]::Combine($work, "config.json")
$cfgJson = '{"title":"Shell PoC","width":640,"height":480,"resizable":true,"fullscreen":false,"entry":"index.html"}'
[IO.File]::WriteAllText($config, $cfgJson)

# ---------------------------------------------------------------- build output

$outDir = Join-Path $work "out"
New-Item -ItemType Directory -Path $outDir | Out-Null

if ($Mode -eq "folder") {
    # Mode B: exe + game/ + config.json
    $folderOut = Join-Path $outDir "poc-folder"
    New-Item -ItemType Directory -Path $folderOut | Out-Null
    Copy-Item $shellExe (Join-Path $folderOut "game.exe")
    Copy-Item $game (Join-Path $folderOut "game") -Recurse
    Copy-Item $config (Join-Path $folderOut "config.json")
    $target = Join-Path $folderOut "game.exe"
}
else {
    # Mode A: shell exe + [zip] + [config] + footer per docs/format.md
    $zip = Join-Path $work "game.zip"
    Compress-Archive -Path (Join-Path $game "*") -DestinationPath $zip

    $exeBytes = [IO.File]::ReadAllBytes($shellExe)
    $zipBytes = [IO.File]::ReadAllBytes($zip)
    $cfgBytes = [IO.File]::ReadAllBytes($config)
    # magic: "HTEX1" + 3 NUL bytes = b"HTEX1\x00\x00\x00"
    $magic = [byte[]]@(72, 84, 69, 88, 49, 0, 0, 0)

    $target = Join-Path $outDir "poc-single.exe"
    $ms = New-Object IO.MemoryStream
    $ms.Write($exeBytes, 0, $exeBytes.Length)
    $ms.Write($zipBytes, 0, $zipBytes.Length)
    $ms.Write($cfgBytes, 0, $cfgBytes.Length)
    $footer = New-Object IO.MemoryStream
    $footer.Write([BitConverter]::GetBytes([uint32]$cfgBytes.Length), 0, 4)
    $footer.Write([BitConverter]::GetBytes([uint32]$zipBytes.Length), 0, 4)
    $footer.Write($magic, 0, 8)
    $footerBytes = $footer.ToArray()
    $ms.Write($footerBytes, 0, $footerBytes.Length)
    [IO.File]::WriteAllBytes($target, $ms.ToArray())
}

$singleSize = (Get-Item $target).Length
$exeOut = Join-Path $outDir "stderr.txt"

# ---------------------------------------------------------------- launch check

$proc = Start-Process -FilePath $target -PassThru -RedirectStandardError $exeOut
Start-Sleep -Seconds 6
$alive = -not $proc.HasExited
if (-not $alive) {
    Write-Host "RESULT $Mode`: FAILED - exited early with code $($proc.ExitCode)"
    $stderr = (Get-Content $exeOut -Raw -ErrorAction SilentlyContinue)
    if ($stderr) { Write-Host $stderr }
    exit 1
}
Stop-Process -Id $proc.Id -Force
Start-Sleep -Milliseconds 500

Write-Host ("RESULT {0}: PASSED - alive after 6s, output {1:N0} KB" -f $Mode, ($singleSize / 1KB))
Write-Host "out dir: $outDir"
