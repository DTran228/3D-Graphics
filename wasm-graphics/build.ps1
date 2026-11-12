$ErrorActionPreference = 'Stop'
$toolchain = 'nightly-2024-08-02'
$bindingVersion = '0.2.100'
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS

Push-Location $PSScriptRoot
try {
    $installed = & wasm-bindgen --version
    if ($LASTEXITCODE -ne 0 -or $installed -ne "wasm-bindgen $bindingVersion") {
        throw "Install the matching CLI with: cargo install wasm-bindgen-cli --version $bindingVersion --locked"
    }

    # Encoded flags preserve the quotes around the getrandom backend on Windows.
    $env:CARGO_ENCODED_RUSTFLAGS = @(
        '--cfg', 'getrandom_backend="wasm_js"'
        '-C', 'target-feature=+atomics,+bulk-memory'
        '-C', 'link-arg=--max-memory=4294967296'
    ) -join [char]31

    & rustup run $toolchain cargo build --locked --release --target wasm32-unknown-unknown -Z build-std=panic_abort,std
    if ($LASTEXITCODE -ne 0) { throw 'Rust WASM build failed.' }

    & wasm-bindgen target/wasm32-unknown-unknown/release/wasm_graphics.wasm --out-dir ../web/wasm --target web --typescript
    if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen failed.' }
}
finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    Pop-Location
}
