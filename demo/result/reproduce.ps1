$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot 'project')
& python bug.py
exit $LASTEXITCODE
