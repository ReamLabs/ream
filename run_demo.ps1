$env_lines = cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && set'
$include_val = ""
foreach ($line in $env_lines) {
    if ($line -like "INCLUDE=*") {
        $include_val = $line.Substring(8)
        break
    }
}
$clang_args = @()
if ($include_val) {
    foreach ($path in $include_val.Split(";")) {
        if ($path.Trim()) {
            $clang_args += "-I`"$($path.Trim())`""
        }
    }
}
$env:BINDGEN_EXTRA_CLANG_ARGS = $clang_args -join " "
$env:LIBCLANG_PATH = 'C:\Users\valok\AppData\Roaming\Python\Python313\site-packages\clang\native'
Write-Host "BINDGEN_EXTRA_CLANG_ARGS: $env:BINDGEN_EXTRA_CLANG_ARGS"
Write-Host "LIBCLANG_PATH: $env:LIBCLANG_PATH"
cargo run --bin reth-poc
