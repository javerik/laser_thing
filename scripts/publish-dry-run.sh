#!/usr/bin/env bash
# Packages every crate and npm package without uploading. Run before publishing:
#   cargo publish --workspace               (core, render, egui in dependency order)
#   npm publish -w @javerik/laserline
#   npm publish -w @javerik/laserline-angular   (from packages/laserline-angular/dist)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)"
for p in packages/laserline packages/laserline-angular; do
    v="$(node -p "require('./$p/package.json').version")"
    if [[ "$v" != "$version" ]]; then
        echo "$p is at $v, the crates at $version: keep them equal" >&2
        exit 1
    fi
done

echo "== crates.io"
cargo publish --dry-run --workspace "$@"

echo "== npm: @javerik/laserline"
scripts/build-wasm.sh
npm run build -w @javerik/laserline
npm test -w @javerik/laserline
(cd packages/laserline && npm publish --dry-run)

echo "== npm: @javerik/laserline-angular"
npx ng build laserline-angular
(cd packages/laserline-angular/dist && npm publish --dry-run)
