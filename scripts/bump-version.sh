#!/bin/bash
# Updates both extension/manifest.json and backend/Cargo.toml to the
# same version number in one step, since Cargo.toml's version has no
# real functional effect anywhere - this is purely for keeping your
# own bookkeeping consistent, not because anything depends on it.
#
# Usage:
# 1. sed -i 's/\r$//' bump-version.sh
# 2. chmod +x bump-version.sh
# 3. ./scripts/bump-version.sh 1.0.2
if [ -z "$1" ]; then
  echo "Usage: ./scripts/bump-version.sh <new-version>"
  echo "Example: ./scripts/bump-version.sh 1.0.2"
  exit 1
fi
NEW_VERSION="$1"

# Finds the project root correctly no matter where this script is
# actually called from - one level up from wherever this file itself
# physically sits.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$PROJECT_ROOT/extension/manifest.json"
sed -i "s/^version = \"[^\"]*\"/version = \"$NEW_VERSION\"/" "$PROJECT_ROOT/backend/Cargo.toml"
echo "Updated both files to version $NEW_VERSION:"
grep "version" "$PROJECT_ROOT/extension/manifest.json" | head -1
grep "^version" "$PROJECT_ROOT/backend/Cargo.toml" | head -1
