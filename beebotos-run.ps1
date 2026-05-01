#!/usr/bin/env pwsh
# BeeBotOS Production Runner (Windows)
# Usage: .\beebotos-run.ps1 [start|stop|restart|status] [gateway|web|beehub|all]

param(
    [string]$Action = "start",
    [string]$Target = "all",
    [string]$WorkingDir = $null
)

$ErrorActionPreference = "Stop"

$ScriptDir = if ($WorkingDir) {
    (Resolve-Path $WorkingDir).Path
} else {
    Split-Path -Parent $MyInvocation.MyCommand.Path
}
Set-Location $ScriptDir

# Ensure data directories exist
$DataDir = Join-Path $ScriptDir "data"
$RunDir = Join-Path $DataDir "run"
$LogDir = Join-Path $DataDir "logs"
New-Item -ItemType Directory -Force -Path $RunDir | Out-Null
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

$Services = @(
    @{ Name = "gateway"; Binary = "target\release\beebotos-gateway.exe"; Port = 8000; Desc = "API Gateway" }
    @{ Name = "web";     Binary = "target\release\web-server.exe";       Port = 8090; Desc = "Web Frontend Server" }
    @{ Name = "beehub";  Binary = "target\release\beehub.exe";           Port = 8080; Desc = "BeeHub Service" }
)

function Test-IsRunning($name) {
    $pidFile = Join-Path $RunDir "$name.pid"
    if (Test-Path $pidFile) {
        $svcPid = Get-Content $pidFile -Raw
        $svcPid = $svcPid.Trim()
        try {
            $proc = Get-Process -Id $svcPid -ErrorAction SilentlyContinue
            if ($proc) { return $true }
        } catch {}
        # 回退：按进程名查找（兼容 MSYS2/bash 启动的进程，其 PID 为模拟层 ID）
        $svc = $Services | Where-Object { $_.Name -eq $name } | Select-Object -First 1
        if ($svc) {
            $binaryName = ($svc.Binary | Split-Path -Leaf) -replace '\.exe$',''
            $procByName = Get-Process | Where-Object { $_.ProcessName -like "*$binaryName*" } | Select-Object -First 1
            if ($procByName) { return $true }
        }
    }
    return $false
}

function Start-ServiceByName($name) {
    $svc = $Services | Where-Object { $_.Name -eq $name } | Select-Object -First 1
    if (-not $svc) {
        Write-Host "Unknown service: $name" -ForegroundColor Red
        return $false
    }

    $binaryPath = Join-Path $ScriptDir $svc.Binary
    if (-not (Test-Path $binaryPath)) {
        # 生产环境回退：二进制与脚本在同一目录
        $localBinary = Join-Path $ScriptDir ($svc.Binary | Split-Path -Leaf)
        if (Test-Path $localBinary) {
            $binaryPath = $localBinary
        } elseif ($name -eq "beehub") {
            Write-Host "BeeHub binary not found, skipping."
            return $true
        } else {
            Write-Host "Binary not found: $binaryPath" -ForegroundColor Red
            return $false
        }
    }

    if (Test-IsRunning $name) {
        $svcPid = (Get-Content (Join-Path $RunDir "$name.pid") -Raw).Trim()
        Write-Host "$($svc.Desc) is already running (PID: $svcPid)" -ForegroundColor Yellow
        return $true
    }

    Write-Host "Starting $($svc.Desc) on port $($svc.Port)..."
    $logFile = Join-Path $LogDir "$name.log"
    $errFile = Join-Path $LogDir "$name.err"
    $pidFile = Join-Path $RunDir "$name.pid"
    $procParams = @{
        FilePath               = $binaryPath
        RedirectStandardOutput = $logFile
        RedirectStandardError  = $errFile
        PassThru               = $true
        WindowStyle            = "Hidden"
    }
    if ($name -eq "web") {
        # 检测生产环境：当前目录已有完整静态资源
        $hasIndexHtml = Test-Path (Join-Path $ScriptDir "index.html")
        $hasPkg = Test-Path (Join-Path $ScriptDir "pkg")
        if ($hasIndexHtml -and $hasPkg) {
            # 生产环境：直接使用当前目录
            $procParams.ArgumentList = @("--static-path", ".", "--gateway-url", "http://localhost:8000")
        } elseif (Test-Path (Join-Path $ScriptDir "apps\web\index.html")) {
            # 开发环境：从 apps/web 准备临时静态目录
            $tempStaticDir = Join-Path $ScriptDir "data\temp-web-static"
            if (Test-Path $tempStaticDir) { Remove-Item -Recurse -Force $tempStaticDir }
            New-Item -ItemType Directory -Force -Path $tempStaticDir | Out-Null
            Copy-Item (Join-Path $ScriptDir "apps\web\index.html") $tempStaticDir
            Copy-Item -Recurse (Join-Path $ScriptDir "apps\web\pkg") $tempStaticDir
            Copy-Item -Recurse (Join-Path $ScriptDir "apps\web\style") $tempStaticDir
            Copy-Item (Join-Path $ScriptDir "apps\web\style\main.css") (Join-Path $tempStaticDir "style.css")
            Copy-Item (Join-Path $ScriptDir "apps\web\style\components.css") (Join-Path $tempStaticDir "components.css")
            $realFavicon = Join-Path $ScriptDir "apps\web\public\favicon.svg"
            if (Test-Path $realFavicon) {
                Copy-Item $realFavicon (Join-Path $tempStaticDir "favicon.svg")
            }
            $markedJs = Join-Path $ScriptDir "apps\web\public\marked.min.js"
            if (Test-Path $markedJs) {
                Copy-Item $markedJs (Join-Path $tempStaticDir "marked.min.js")
            }
            $procParams.ArgumentList = @("--static-path", $tempStaticDir, "--gateway-url", "http://localhost:8000")
        } else {
            Write-Host "Web static files not found. Please build web first: .\beebotos-dev.ps1 build web" -ForegroundColor Red
            return $false
        }
    }
    $proc = Start-Process @procParams
    $proc.Id | Set-Content $pidFile -NoNewline
    Start-Sleep -Seconds 1
    try {
        $check = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
        if ($check) {
            Write-Host "$($svc.Desc) started (PID: $($proc.Id))" -ForegroundColor Green
            return $true
        }
    } catch {}
    Write-Host "$($svc.Desc) failed to start. Check $logFile and $errFile" -ForegroundColor Red
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
    return $false
}

function Stop-ServiceByName($name) {
    $svc = $Services | Where-Object { $_.Name -eq $name } | Select-Object -First 1
    if (-not $svc) {
        Write-Host "Unknown service: $name" -ForegroundColor Red
        return
    }

    $pidFile = Join-Path $RunDir "$name.pid"
    if (-not (Test-IsRunning $name)) {
        Write-Host "$($svc.Desc) is not running" -ForegroundColor Yellow
        Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
        return
    }

    $svcPid = (Get-Content $pidFile -Raw).Trim()
    Write-Host "Stopping $($svc.Desc) (PID: $svcPid)..." -ForegroundColor Cyan
    try {
        Stop-Process -Id $svcPid -Force -ErrorAction Stop
        Write-Host "$($svc.Desc) stopped" -ForegroundColor Green
    } catch {
        Write-Host "Could not stop $($svc.Desc) gracefully: $($_.Exception.Message)" -ForegroundColor Yellow
    }
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}

function Restart-ServiceByName($name) {
    Stop-ServiceByName $name
    Start-Sleep -Seconds 1
    Start-ServiceByName $name | Out-Null
}

function Show-Status {
    Write-Host "Service Status" -ForegroundColor Cyan
    Write-Host "----------------------------------------" -ForegroundColor Cyan
    Write-Host ("{0,-12} {1,-10} {2,-8} {3}" -f "Service", "Status", "PID", "Port")
    Write-Host "----------------------------------------"
    foreach ($svc in $Services) {
        $pidFile = Join-Path $RunDir "$($svc.Name).pid"
        if (Test-IsRunning $svc.Name) {
            $svcPid = (Get-Content $pidFile -Raw).Trim()
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "running", $svcPid, $svc.Port
            Write-Host $line -ForegroundColor Green
        } else {
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "stopped", "-", $svc.Port
            Write-Host $line -ForegroundColor Red
        }
    }
}

switch ($Action) {
    "start" {
        switch ($Target) {
            "gateway" { Start-ServiceByName "gateway" | Out-Null }
            "web"     { Start-ServiceByName "web"     | Out-Null }
            "beehub"  { Start-ServiceByName "beehub"  | Out-Null }
            "all" {
                foreach ($svc in $Services) { Start-ServiceByName $svc.Name | Out-Null }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) start [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "stop" {
        switch ($Target) {
            "gateway" { Stop-ServiceByName "gateway" }
            "web"     { Stop-ServiceByName "web" }
            "beehub"  { Stop-ServiceByName "beehub" }
            "all" {
                foreach ($svc in $Services) { Stop-ServiceByName $svc.Name }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) stop [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "restart" {
        switch ($Target) {
            "gateway" { Restart-ServiceByName "gateway" }
            "web"     { Restart-ServiceByName "web" }
            "beehub"  { Restart-ServiceByName "beehub" }
            "all" {
                foreach ($svc in $Services) { Restart-ServiceByName $svc.Name }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) restart [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "status" { Show-Status }
    default {
        Write-Host "Usage: $($MyInvocation.MyCommand.Name) [start|stop|restart|status] [gateway|web|beehub|all]" -ForegroundColor Red
        exit 1
    }
}
