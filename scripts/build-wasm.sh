#!/usr/bin/env bash
# Builds the wasm bindings into packages/laserline/wasm (wasm-bindgen --target web).
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching the
# wasm-bindgen crate version; wasm-opt (binaryen) is used when installed.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packages/laserline/wasm"
profile="${PROFILE:-wasm-release}"

want="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' "$root/Cargo.lock" | head -n1)"
have="$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)"
if [[ "$have" != "$want" ]]; then
    echo "wasm-bindgen-cli $want required (found: ${have:-none}):" >&2
    echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi

cargo build --manifest-path "$root/Cargo.toml" -p laserline-wasm --target wasm32-unknown-unknown --profile "$profile"
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" --out-name laserline \
    "$root/target/wasm32-unknown-unknown/$profile/laserline.wasm"
if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-reference-types \
        "$out/laserline_bg.wasm" -o "$out/laserline_bg.wasm"
else
    echo "wasm-opt not found: skipping size optimisation" >&2
fi
ls -l "$out"
