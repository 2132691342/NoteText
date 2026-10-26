<#
验证入口：所有验证动作只从这里走。

  pwsh scripts/check.ps1                  日常：rustfmt → cargo check → 全量链路测试
  pwsh scripts/check.ps1 -Test archive    只跑名字含 archive 的链路（改压缩子系统时用）
  pwsh scripts/check.ps1 -Full            提交前：+ i18n 审计 + clippy + 前端构建

只跑一条链路时不做 fmt / clippy / 前端：改一个函数要等整套门禁，就没人愿意频繁验证了。
#>
[CmdletBinding()]
param(
    [string]$Test = '',
    [switch]$Full
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$Log = '.check/test.log'
New-Item -ItemType Directory -Force -Path '.check' | Out-Null

function Invoke-Step {
    param([string]$Name, [scriptblock]$Body)
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   失败：$Name（退出码 $LASTEXITCODE）" -ForegroundColor Red
        if (Test-Path $Log) {
            Get-Content $Log |
                Select-String -Pattern 'FAILED|panicked|^---- |\.rs:\d+' |
                Select-Object -First 20 |
                ForEach-Object { Write-Host $_ }
        }
        exit $LASTEXITCODE
    }
}

if ($Test) {
    Invoke-Step "cargo test（$Test）" {
        cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests -- $Test 2>&1 |
            Tee-Object -FilePath $Log | Out-Null
    }
    Write-Host "`n通过" -ForegroundColor Green
    exit 0
}

if ($Full) {
    if (-not (Test-Path 'frontend/node_modules')) {
        Invoke-Step 'npm ci' {
            Push-Location frontend
            try { npm ci } finally { Pop-Location }
        }
    }
    Invoke-Step 'i18n' { node scripts/i18n-audit.mjs }
    Invoke-Step 'clippy' {
        cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    }
    Invoke-Step 'frontend build' {
        Push-Location frontend
        try { npm run build } finally { Pop-Location }
    }
}

# rustfmt 必须无输出：.gitattributes 把 *.rs 钉成 LF，CRLF 会被一律报成未格式化。
Invoke-Step 'rustfmt' { cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check }
Invoke-Step 'cargo check' { cargo check --manifest-path src-tauri/Cargo.toml --all-targets }
Invoke-Step 'cargo test' {
    cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests 2>&1 |
        Tee-Object -FilePath $Log | Out-Null
}

Write-Host "`n通过" -ForegroundColor Green
