$lock = Get-Content Cargo.lock -Raw
$matches = [regex]::Matches($lock, '(?ms)\[\[package\]\]\r?\nname = "alloy-[^"]+"\r?\nversion = "[^"]+"')
foreach ($match in $matches) {
    Write-Host $match.Value
}
