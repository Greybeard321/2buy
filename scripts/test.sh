#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"

cargo test "$@"
node --test tests/frontend.test.mjs
