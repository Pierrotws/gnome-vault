#!/usr/bin/env bash
#
# release.sh — cut a release end-to-end.
#
# Usage:  ./release.sh <version>     e.g.  ./release.sh 0.1.1
#
# Steps performed (each requires the previous to succeed):
#   1. Bump version in Cargo.toml + meson.build, regenerate Cargo.lock.
#   2. Commit "bump to version $VERSION" and push to origin/main.
#   3. Create the GitHub release (triggers .github/workflows/packages.yml).
#      Wait for the workflow to finish so the release tarball/.deb exist.
#   4. Bump pkgver in both PKGBUILDs and rewrite gnome-vault-bin's
#      sha256sums against the just-published tarball; commit + push.
#   5. Mirror each PKGBUILD into the matching AUR clone, regenerate
#      .SRCINFO, commit, and push to ssh://aur@aur.archlinux.org.
#
# Prerequisites:
#   - clean working tree on main, ahead of origin = 0
#   - gh CLI authenticated (gh auth status)
#   - SSH access to aur@aur.archlinux.org
#   - $AUR_ROOT (default ~/aur) writable; existing clones are pulled --ff-only

set -euo pipefail

# --- Args ----------------------------------------------------------------

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <version>     e.g. $0 0.1.1" >&2
    exit 64
fi

VERSION="$1"
TAG="v${VERSION}"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Version must be MAJOR.MINOR.PATCH (got: $VERSION)" >&2
    exit 64
fi

# --- Sanity checks -------------------------------------------------------

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
    echo "Working tree is not clean. Commit or stash before releasing." >&2
    exit 1
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$CURRENT_BRANCH" != "main" ]]; then
    echo "Not on main (on $CURRENT_BRANCH). Switch first." >&2
    exit 1
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo "Tag $TAG already exists locally. Aborting." >&2
    exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
    echo "gh CLI not authenticated. Run: gh auth login" >&2
    exit 1
fi

for cmd in cargo curl sha256sum makepkg sed awk; do
    command -v "$cmd" >/dev/null || { echo "Missing required tool: $cmd" >&2; exit 1; }
done

AUR_ROOT="${AUR_ROOT:-$HOME/aur}"
mkdir -p "$AUR_ROOT"

OWNER_REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"

# --- Step 1: bump versions in source ------------------------------------

echo "==> Bumping Cargo.toml and meson.build to $VERSION"
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
sed -i "s/^  version: '[^']*',/  version: '$VERSION',/" meson.build

# Regenerate Cargo.lock so the workspace version field matches.
cargo check --quiet

# --- Step 2: commit & push ----------------------------------------------

echo "==> Committing version bump"
git add Cargo.toml Cargo.lock meson.build
git commit -m "bump to version $VERSION"
git push origin main

# --- Step 3: GitHub release + wait for workflow -------------------------

echo "==> Creating GitHub release $TAG"
gh release create "$TAG" --title "$TAG" --generate-notes

echo "==> Waiting for the packages workflow to register..."
sleep 15

RUN_ID="$(gh run list \
    --workflow=packages.yml \
    --event=release \
    --limit=1 \
    --json databaseId \
    --jq '.[0].databaseId')"

if [[ -z "$RUN_ID" ]]; then
    echo "Could not find a release-triggered run of packages.yml." >&2
    echo "Check 'gh run list --workflow=packages.yml' and rerun the rest manually." >&2
    exit 1
fi

echo "==> Watching workflow run $RUN_ID"
gh run watch "$RUN_ID" --exit-status

# --- Step 4: bump PKGBUILDs against the new tarball ---------------------

TARBALL_URL="https://github.com/${OWNER_REPO}/releases/download/${TAG}/gnome-vault-${VERSION}-x86_64.tar.gz"
echo "==> Hashing released tarball: $TARBALL_URL"
HASH="$(curl -sLf "$TARBALL_URL" | sha256sum | awk '{print $1}')"

if [[ -z "$HASH" || "$HASH" == "d41d8cd98f00b204e9800998ecf8427e" ]]; then
    echo "Tarball hash empty or matches the empty-file sentinel; aborting." >&2
    exit 1
fi

echo "==> Updating packaging/arch/*/PKGBUILD"
for pkg in gnome-vault gnome-vault-bin; do
    pkgbuild="packaging/arch/$pkg/PKGBUILD"
    sed -i "s/^pkgver=.*/pkgver=$VERSION/" "$pkgbuild"
    sed -i "s/^pkgrel=.*/pkgrel=1/" "$pkgbuild"
done
sed -i "s/^sha256sums=('[^']*')/sha256sums=('$HASH')/" \
    packaging/arch/gnome-vault-bin/PKGBUILD

git add packaging/arch/
git commit -m "package: bump PKGBUILDs to $VERSION"
git push origin main

# --- Step 5: sync to AUR ------------------------------------------------

for pkg in gnome-vault gnome-vault-bin; do
    aur_dir="$AUR_ROOT/$pkg"
    if [[ ! -d "$aur_dir/.git" ]]; then
        echo "==> Cloning AUR repo for $pkg into $aur_dir"
        git clone "ssh://aur@aur.archlinux.org/$pkg.git" "$aur_dir"
    else
        echo "==> Pulling AUR repo for $pkg"
        git -C "$aur_dir" pull --ff-only
    fi

    cp "$REPO_ROOT/packaging/arch/$pkg/PKGBUILD" "$aur_dir/PKGBUILD"

    (
        cd "$aur_dir"
        makepkg --printsrcinfo > .SRCINFO
        git add PKGBUILD .SRCINFO
        git commit -m "Update to v$VERSION"
        git push origin master
    )
done

echo "==> Released $TAG. Both AUR packages synced."
