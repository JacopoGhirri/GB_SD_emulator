#!/usr/bin/env bash
# Download the pinned, freely redistributable test-ROM bundle into tests/roms/.
#
# Source: https://github.com/c-sp/game-boy-test-roms (MIT licence for the collection;
# each contained suite keeps its own licence, see the README files inside the bundle).
#
# Pinned: a fixed release tag plus a SHA-256 of the archive, so every run and every CI
# job gets byte-identical ROMs. To upgrade, change VERSION and SHA256 together and
# record the change in docs/REFERENCES.md and PROGRESS.md.
#
# Behaviour:
#   - Idempotent: if the bundle is already extracted and verified, nothing is downloaded.
#   - Offline (no network): prints instructions and exits 0, so tests skip instead of fail.
#   - Checksum mismatch: deletes the download and exits 1. Never keep an unverified archive.

set -u

VERSION="v7.0"
ARCHIVE="game-boy-test-roms-${VERSION}.zip"
URL="https://github.com/c-sp/game-boy-test-roms/releases/download/${VERSION}/${ARCHIVE}"
SHA256="B9A9D7A1075AA35A3D07C07C34974048672D8520DCA9E07A50178F5860C3832C"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ROMS_DIR="${REPO_ROOT}/tests/roms"
DEST="${ROMS_DIR}/game-boy-test-roms-${VERSION}"
MARKER="${DEST}/.verified-sha256"
TMP_ZIP="${ROMS_DIR}/${ARCHIVE}.part"

say() { printf '%s\n' "$*"; }

# Already done for this exact pinned archive? Then we are finished.
if [ -f "${MARKER}" ] && [ "$(cat "${MARKER}")" = "${SHA256}" ]; then
    say "test ROMs already present and verified: ${DEST}"
    exit 0
fi

mkdir -p "${ROMS_DIR}"

say "downloading ${URL}"
if ! curl --fail --silent --show-error --location --output "${TMP_ZIP}" "${URL}"; then
    rm -f "${TMP_ZIP}"
    say ""
    say "could not download the test ROM bundle (offline or GitHub unreachable)."
    say "tests that need these ROMs will be skipped."
    say "to fetch them later, run this script again with network access, or download"
    say "${URL}"
    say "and extract it into ${DEST}/ (the archive contents go directly in that folder)."
    exit 0
fi

ACTUAL="$(sha256sum "${TMP_ZIP}" | cut -d' ' -f1)"
EXPECTED_LOWER="$(printf '%s' "${SHA256}" | tr 'A-F' 'a-f')"
if [ "${ACTUAL}" != "${EXPECTED_LOWER}" ]; then
    rm -f "${TMP_ZIP}"
    say "error: SHA-256 mismatch for ${ARCHIVE}"
    say "  expected ${EXPECTED_LOWER}"
    say "  got      ${ACTUAL}"
    say "the download was deleted. Do not extract an archive that fails verification."
    exit 1
fi
say "checksum OK (${ACTUAL})"

rm -rf "${DEST}"
mkdir -p "${DEST}"
if ! unzip -q "${TMP_ZIP}" -d "${DEST}"; then
    rm -f "${TMP_ZIP}"
    say "error: unzip failed; partial files left in ${DEST} were removed"
    rm -rf "${DEST}"
    exit 1
fi
rm -f "${TMP_ZIP}"

ROM_COUNT="$(find "${DEST}" -name '*.gb' -o -name '*.gbc' | wc -l | tr -d ' ')"
if [ "${ROM_COUNT}" -eq 0 ]; then
    say "error: archive extracted but contains no .gb/.gbc files"
    exit 1
fi

printf '%s' "${SHA256}" > "${MARKER}"
say "extracted ${ROM_COUNT} ROM files into ${DEST}"
