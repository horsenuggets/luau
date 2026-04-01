#!/usr/bin/env bash
set -euo pipefail
VERSION=$(grep 'set(LUAU_VERSION' CMakeLists.txt | sed 's/.*"\(.*\)".*/\1/')
echo $VERSION
