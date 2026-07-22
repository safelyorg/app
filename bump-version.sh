#!/bin/bash
# Updates both extension/manifest.json and backend/Cargo.toml to the
# same version number in one step, since Cargo.toml's version has no
# real functional effect anywhere - this is purely for keeping your
# own bookkeeping consistent, not because anything depends on it.
#
# Usage:
# 1. sed -i 's/\r$//' bump-version.sh
# 2. chmod +x bump-version.sh
# 3. ./bump-version.sh 1.0.2

if [ -z "$1" ]; then
  echo "Usage: ./bump-version.sh <new-version>"
  echo "Example: ./bump-version.sh 1.0.2"
  exit 1
fi

NEW_VERSION="$1"

sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" extension/manifest.json
sed -i "s/^version = \"[^\"]*\"/version = \"$NEW_VERSION\"/" backend/Cargo.toml

echo "Updated both files to version $NEW_VERSION:"
grep "version" extension/manifest.json | head -1
grep "^version" backend/Cargo.toml | head -1
