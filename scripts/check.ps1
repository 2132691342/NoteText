<#
验证入口：一个脚本收口全部验证动作，按「改了多大」选档。

  pwsh scripts/check.ps1              快门：格式 → 编译 → 全量测试（日常）
  pwsh scripts/check.ps1 -Chain 行尾   只跑名字命中的那条链
  pwsh scripts/check.ps1 -List        列出全部测试链
  pwsh scripts/check.ps1 -Full        全量门禁：+ clippy + 前端构建（提交前 / CI）

档位怎么选：耗时大头是编译与链接，不是断言——-Chain 省下的正是那一段。
-List 靠命名查找：入口一律叫 test_<domain>_chain。

前端必须先于 Rust 编译就位：tauri.conf.json 的 frontendDist 指向 frontend/dist，
而 dist 被 .gitignore 排除，全新 checkout 上没有。
#>
[CmdletBinding()]
param(
    [switch]$Full,
    [string]$Chain,
    [switch]$List,
    [switch]$Detail
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$TestLog = '.check/test-output.log'
if (-not (Test-Path '.check')) { New-Item -ItemType Directory -Path '.check' | Out-Null }

function Invoke-Step {
    param([string]$Name, [scriptblock]$Body)
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "    FAILED: $Name（退出码 $LASTEXITCODE）" -ForegroundColor Red
        Write-FailureSummary
        exit $LASTEXITCODE
    }
}

# 失败时从日志里摘出可定位的片段：测试名是第一个把手，.rs:行号 直接指到断言。
function Write-FailureSummary {
    if (-not (Test-Path $TestLog)) { return }
    $log = Get-Content $TestLog -ErrorAction SilentlyContinue
    if (-not $log) { return }
    $marks = $log | Select-String -Pattern '^test .* FAILED|^---- |^thread .* panicked|\.rs:\d+:' -Context 0, 4
    $body = if ($marks) { ($marks | Out-String) } else { ($log | Select-Object -Last 30) -join "`n" }
    if (-not $body.Trim()) { return }
    Write-Host ''
    Write-Host '=== 失败摘要 ===' -ForegroundColor Red
    Write-Host $body
    if ($env:GITHUB_STEP_SUMMARY) {
        $fence = ([string][char]96) * 3
        Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "## 验证失败摘要`n`n${fence}text`n$body`n${fence}" -Encoding utf8
    }
}

# 测试入口：src-tauri/tests/ 下按域分的文件。
# 一个域一个文件，改某个域只读那个文件，不必通读全部测试。
$TestDir = 'src-tauri/tests'

if ($List) {
    Get-ChildItem -Path $TestDir -Filter '*_chain.rs' | Sort-Object Name | ForEach-Object {
        Write-Host $_.Name -ForegroundColor Cyan
        Get-Content $_.FullName |
            Select-String -Pattern '^fn (test_\w+)\(' |
            ForEach-Object { '  {0}' -f $_.Matches[0].Groups[1].Value }
    }
    exit 0
}

if ($Full -and $Chain) {
    throw '-Chain 是定点定位，-Full 是全量门禁，二选一。'
}

# rustfmt 必须无输出：.gitattributes 把 *.rs 钉成 LF，CRLF 会被一律报成未格式化。
Invoke-Step 'rustfmt' {
    $bad = @(cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check 2>&1 |
        Where-Object { $_ -and $_ -notmatch '^Diff in' })
    foreach ($f in $bad) { Write-Host "      需要格式化：$f" -ForegroundColor Yellow }
    $global:LASTEXITCODE = [int][bool]$bad
}

if ($Full -or -not (Test-Path 'frontend/dist/index.html')) {
    if (-not (Test-Path 'frontend/node_modules')) {
        Invoke-Step 'frontend deps' {
            Push-Location frontend
            try { npm ci } finally { Pop-Location }
        }
    }
    Invoke-Step 'frontend build' {
        Push-Location frontend
        try { npm run build } finally { Pop-Location }
    }
}

# 编译比测试更早暴露类型错误，覆盖全部包（含没有测试的）。
Invoke-Step 'cargo check' {
    cargo check --manifest-path src-tauri/Cargo.toml --all-targets
}

if ($Full) {
    Invoke-Step 'clippy' {
        cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    }
}

# -Chain 可只给片段：拼一个空前缀让「行尾」不必知道它挂在哪个入口下。
if ($Chain) {
    $filter = if ($Chain -match '^test_\w+chain') { $Chain } else { $Chain }
    Invoke-Step "cargo test $Chain" {
        cargo test --manifest-path src-tauri/Cargo.toml -- $filter 2>&1 |
            Tee-Object -FilePath $TestLog | Out-Null
    }
}
else {
    Invoke-Step 'cargo test' {
        cargo test --manifest-path src-tauri/Cargo.toml 2>&1 |
            Tee-Object -FilePath $TestLog | Out-Null
        if ($Detail) { Get-Content $TestLog | Select-String -Pattern '^test ' }
    }
}

Write-Host ''
Write-Host '全部通过' -ForegroundColor Green
