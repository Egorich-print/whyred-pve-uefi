#!/usr/bin/env bash
# One command to validate the repo: rust fmt/clippy/tests, shell + python
# syntax, and the dist manifest. No device access, no network, no VM.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

echo "== rust: fmt =="
( cd tools && cargo fmt --all -- --check )
echo "== rust: clippy =="
( cd tools && cargo clippy --workspace --all-targets --locked -- -D warnings )
echo "== rust: test =="
( cd tools && cargo test --workspace --locked )
if [ -f apps/unlocker/src-tauri/Cargo.toml ]; then
    echo "== tauri: fmt/clippy/test =="
    ( cd apps/unlocker/src-tauri \
        && cargo fmt --all -- --check \
        && cargo clippy --workspace --all-targets --locked -- -D warnings \
        && cargo test --workspace --locked \
        && cargo test -p mibox-core --lib )   # path dep, not a workspace member
fi

echo "== shell syntax =="
for f in flash_all.sh scripts/*.sh edk2/*.sh pve/*.sh; do bash -n "$f"; done
sh -n scripts/lavender-boot-fastboot.sh

echo "== python syntax =="
python3 -B -m py_compile tools/*.py

echo "== dist manifest =="
if [ -f dist/SHA256SUMS ]; then
    ( cd dist && shasum -a 256 -c SHA256SUMS )
else
    echo "dist/SHA256SUMS absent — run scripts/make-dist.sh after a build"
fi

echo "ALL CHECKS PASSED"
