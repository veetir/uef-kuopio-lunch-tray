#!/bin/bash

# LunchTray installer / updater for macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/veetir/uef-kuopio-lunch-tray/master/install.sh | bash

set -euo pipefail

repo="veetir/uef-kuopio-lunch-tray"
install_dir="${LUNCHTRAY_INSTALL_DIR:-/Applications}"
target="$install_dir/LunchTray.app"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/lunchtray-install.XXXXXX")"
stage="$install_dir/.LunchTray.app.install.$$"
backup="$install_dir/.LunchTray.app.backup.$$"

cleanup() {
    rm -rf "$stage" "$tmp"
    if [[ -d "$backup" ]]; then
        if [[ -e "$target" ]]; then
            rm -rf "$backup"
        elif ! mv "$backup" "$target"; then
            echo "Could not restore the previous app from $backup." >&2
        fi
    fi
}
trap cleanup EXIT INT TERM

macos_major="$(sw_vers -productVersion | cut -d. -f1)"
if [[ ! "$macos_major" =~ ^[0-9]+$ || "$macos_major" -lt 13 ]]; then
    echo "LunchTray requires macOS 13 or newer." >&2
    exit 1
fi

echo "Looking up the latest LunchTray release..."
curl -fsSL \
    --retry 3 \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: LunchTray-macOS-Installer" \
    "https://api.github.com/repos/$repo/releases?per_page=100" \
    -o "$tmp/releases.json"

release_info="$(/usr/bin/osascript -l JavaScript - "$tmp/releases.json" <<'JXA'
ObjC.import("Foundation");

function run(argv) {
    const data = $.NSData.dataWithContentsOfFile(argv[0]);
    if (!data) throw new Error("Could not read the GitHub releases response.");
    const text = ObjC.unwrap(
        $.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding)
    );
    const pattern = /^macos-v(\d+)\.(\d+)\.(\d+)$/;
    const releases = JSON.parse(text)
        .filter(release => !release.draft && !release.prerelease)
        .map(release => {
            const match = pattern.exec(release.tag_name || "");
            return match
                ? { release, version: match.slice(1).map(Number) }
                : null;
        })
        .filter(Boolean)
        .sort((left, right) => {
            for (let index = 0; index < 3; index += 1) {
                if (left.version[index] !== right.version[index]) {
                    return right.version[index] - left.version[index];
                }
            }
            return 0;
        });
    if (!releases.length) throw new Error("No published macOS release found.");

    const latest = releases[0].release;
    const zip = latest.assets.find(asset => asset.name === "LunchTray.zip");
    const checksum = latest.assets.find(
        asset => asset.name === "LunchTray.zip.sha256"
    );
    if (!zip || !checksum) {
        throw new Error(
            `Release ${latest.tag_name} is missing its macOS assets.`
        );
    }
    return [
        latest.tag_name,
        zip.browser_download_url,
        checksum.browser_download_url
    ].join("\n");
}
JXA
)"

release_tag="$(printf '%s\n' "$release_info" | sed -n '1p')"
zip_url="$(printf '%s\n' "$release_info" | sed -n '2p')"
checksum_url="$(printf '%s\n' "$release_info" | sed -n '3p')"
if [[ -z "$release_tag" || -z "$zip_url" || -z "$checksum_url" ]]; then
    echo "Could not determine the latest macOS release." >&2
    exit 1
fi

echo "Downloading $release_tag..."
curl -fsSL --retry 3 "$zip_url" -o "$tmp/LunchTray.zip"
curl -fsSL --retry 3 "$checksum_url" -o "$tmp/LunchTray.zip.sha256"

(
    cd "$tmp"
    /usr/bin/shasum -a 256 -c LunchTray.zip.sha256
)

mkdir "$tmp/extracted"
/usr/bin/ditto -x -k "$tmp/LunchTray.zip" "$tmp/extracted"
source_app="$tmp/extracted/LunchTray.app"
plist="$source_app/Contents/Info.plist"
if [[ ! -d "$source_app" || ! -f "$plist" ]]; then
    echo "The downloaded archive did not contain LunchTray.app." >&2
    exit 1
fi

bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")"
if [[ "$bundle_id" != "io.github.veetir.CompassLunch" ]]; then
    echo "The downloaded app has an unexpected bundle identifier: $bundle_id" >&2
    exit 1
fi
/usr/bin/codesign --verify --deep --strict "$source_app"

mkdir -p "$install_dir"
if [[ ! -w "$install_dir" ]]; then
    echo "$install_dir is not writable by the current user." >&2
    echo "Install manually or set LUNCHTRAY_INSTALL_DIR to a writable Applications folder." >&2
    exit 1
fi

if [[ -e "$target" ]]; then
    /usr/bin/pkill -x LunchTray 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        /usr/bin/pgrep -x LunchTray >/dev/null 2>&1 || break
        sleep 0.2
    done
    if /usr/bin/pgrep -x LunchTray >/dev/null 2>&1; then
        echo "LunchTray is still running. Quit it and run the installer again." >&2
        exit 1
    fi
fi

echo "Installing $release_tag to $target"
/usr/bin/ditto "$source_app" "$stage"
/usr/bin/xattr -dr com.apple.quarantine "$stage"
/usr/bin/codesign --verify --deep --strict "$stage"

if [[ -e "$target" ]]; then
    mv "$target" "$backup"
fi
mv "$stage" "$target"

if [[ "${LUNCHTRAY_SKIP_LAUNCH:-0}" != "1" ]]; then
    /usr/bin/open "$target"
fi

echo "LunchTray $release_tag is installed in $target."
