#!/bin/bash
# Script to prepare a new release

set -e

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 v1.0.0"
  exit 1
fi

VERSION=$1

# Validate version format
if ! [[ $VERSION =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Version must follow the format vX.Y.Z (e.g., v1.0.0)"
  exit 1
fi

echo "🚀 Preparing release $VERSION..."

# Make sure we're on the main branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" != "main" ]; then
  echo "Error: You must be on the main branch to create a release"
  exit 1
fi

# Make sure the working directory is clean
if ! git diff-index --quiet HEAD --; then
  echo "Error: Working directory has uncommitted changes"
  exit 1
fi

# Pull latest changes
echo "📋 Pulling latest changes from main..."
git pull origin main

# Update version in Cargo.toml
echo "📋 Updating version in Cargo.toml..."
sed -i.bak -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"${VERSION#v}\"/" Cargo.toml
rm Cargo.toml.bak

# Run CI checks
echo "📋 Running CI checks..."
./scripts/ci-check.sh

# Commit version change
echo "📋 Committing version change..."
git add Cargo.toml
git commit -m "Bump version to $VERSION"

# Create a tag
echo "📋 Creating git tag..."
git tag -a "$VERSION" -m "Release $VERSION"

echo "✅ Release $VERSION prepared!"
echo ""
echo "Next steps:"
echo "1. Push the changes and tag: git push origin main --tags"
echo "2. Create a new release on GitHub with tag $VERSION"
echo "3. The CI/CD workflow will build and publish the release"