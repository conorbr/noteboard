#!/bin/sh
# Write Formula/noteboard.rb in the Homebrew tap for a published release.
#
# Usage:
#   scripts/update-homebrew.sh 0.4.0                 # tap at ~/Developer/homebrew-tap
#   scripts/update-homebrew.sh 0.4.0 path/to/tap
#
# It downloads the four .sha256 files the Release workflow attached to the
# GitHub Release and writes a formula that installs those prebuilt binaries.
# Review, commit and push the tap yourself afterwards.

set -eu

REPO="conorbr/noteboard"
version="${1:?usage: $0 VERSION [TAP_DIR]}"
version="${version#v}"
tap="${2:-$HOME/Developer/homebrew-tap}"
formula="$tap/Formula/noteboard.rb"

[ -d "$tap" ] || {
  echo "no tap at $tap" >&2
  exit 1
}

# NOTEBOARD_RELEASES_URL points at a mirror (or a local server, for testing).
base="${NOTEBOARD_RELEASES_URL:-https://github.com/$REPO/releases}/download/v$version"
sha() {
  curl -fsSL "$base/noteboard-v$version-$1.tar.gz.sha256" | cut -d' ' -f1
}

mac_arm=$(sha aarch64-apple-darwin)
mac_intel=$(sha x86_64-apple-darwin)
linux_arm=$(sha aarch64-unknown-linux-musl)
linux_intel=$(sha x86_64-unknown-linux-musl)

for s in "$mac_arm" "$mac_intel" "$linux_arm" "$linux_intel"; do
  case "$s" in
    [0-9a-f]*) [ ${#s} -eq 64 ] || { echo "bad checksum '$s'" >&2; exit 1; } ;;
    *) echo "bad checksum '$s' (is v$version released?)" >&2; exit 1 ;;
  esac
done

mkdir -p "$tap/Formula"
cat >"$formula" <<EOF
class Noteboard < Formula
  desc "Kanban board for a folder of Markdown tickets, in the terminal"
  homepage "https://github.com/$REPO"
  license "MIT"

  # Prebuilt binaries from the GitHub Release. Nothing to compile.
  on_macos do
    on_arm do
      url "$base/noteboard-v$version-aarch64-apple-darwin.tar.gz"
      sha256 "$mac_arm"
    end
    on_intel do
      url "$base/noteboard-v$version-x86_64-apple-darwin.tar.gz"
      sha256 "$mac_intel"
    end
  end

  on_linux do
    on_arm do
      url "$base/noteboard-v$version-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$base/noteboard-v$version-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "noteboard"
    generate_completions_from_executable(bin/"noteboard", "completions")
  end

  test do
    assert_match "noteboard #{version}", shell_output("#{bin}/noteboard --version")

    system bin/"noteboard", "-C", testpath, "add", "Brew test", "--tag", "ci"
    assert_match "brew-test", shell_output("#{bin}/noteboard -C #{testpath} ls")
    assert_path_exists testpath/"brew-test.md"
  end
end
EOF

echo "wrote $formula for noteboard $version"
echo "next: cd $tap && git add Formula/noteboard.rb && git commit -m 'noteboard $version'"
