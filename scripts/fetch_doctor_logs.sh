#!/usr/bin/env bash
# Download Gameboy Doctor's known-good execution logs for Blargg cpu_instrs 01..11.
#
# Source: https://github.com/robert/gameboy-doctor (MIT licence), pinned to a commit.
# Each zip is verified against a SHA-256 before extraction. The hashes were computed by hand
# from the first download (trust on first use), on 2026-11-13.
# Output: tests/roms/gameboy-doctor/cpu_instrs/N.log  (gitignored, like all of tests/roms/).
#
# Idempotent. Offline: prints a note and exits 0, so the trace test skips instead of failing.
# Checksum mismatch: deletes the download and exits 1.

set -u

COMMIT="cc742777af7d71de3061c65733ecfa250237012b"
BASE="https://raw.githubusercontent.com/robert/gameboy-doctor/${COMMIT}/truth/zipped/cpu_instrs"

# index:sha256 of N.zip
HASHES=(
  "1:8e9f36db0a7196f222d7c526da4bcb73d38ce42ccef68ef1a73672db3875d32f"
  "2:937ab150bb823688b4bbd1cd5740263e2855205bea5c6140dc1e5b0f87424bc5"
  "3:6435e1360d32691dd77ed210b1ebe1ee2bf404911cc3d52df79c6a557563e13b"
  "4:092370f82b62cc646488c4157377d1fb62851718380a25bd1f8ee1d522395d74"
  "5:6e7027a872fb292535967156d16085b9b42d20ab28fd5bfc6eef870305f2389a"
  "6:1642c2c15984b7cd1eade83b37cf0fe6ad334ff83f00c31ada2c323668a69116"
  "7:6ea9fe9063ad0e0342c4ed129170be137924abad27fd0ade51b736c2552b2000"
  "8:e093c6f13a37271c357886a15b572be5be8d04a86b610fabc8e75d5bdc8aae6c"
  "9:4e4df62a097ebd348877fac4dd553c541b6c873b6a31d308a85affaab1a71592"
  "10:d6dfd53e8df92cd07dd88f5b32e2c6b0fc4fd6777da9d2848c9aa1f010f97ac2"
  "11:d1eb0b504da593b68cb1d9954fcc97ec15c7d7a42a92cc61e3198c7b2c447f5b"
)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$(cd "${SCRIPT_DIR}/.." && pwd)/tests/roms/gameboy-doctor/cpu_instrs"
mkdir -p "${DEST}"

for entry in "${HASHES[@]}"; do
    n="${entry%%:*}"
    want="${entry#*:}"
    if [ -f "${DEST}/${n}.log" ] && [ -f "${DEST}/${n}.sha256" ] && [ "$(cat "${DEST}/${n}.sha256")" = "${want}" ]; then
        continue
    fi
    zip="${DEST}/${n}.zip.part"
    if ! curl --fail --silent --show-error --location --output "${zip}" "${BASE}/${n}.zip"; then
        rm -f "${zip}"
        echo "could not download Doctor log ${n} (offline?). The trace test will be skipped."
        exit 0
    fi
    got="$(sha256sum "${zip}" | cut -d' ' -f1)"
    if [ "${got}" != "${want}" ]; then
        rm -f "${zip}"
        echo "error: SHA-256 mismatch for Doctor log ${n}.zip"
        echo "  expected ${want}"
        echo "  got      ${got}"
        exit 1
    fi
    if ! unzip -q -o "${zip}" -d "${DEST}"; then
        rm -f "${zip}"
        echo "error: unzip failed for ${n}.zip"
        exit 1
    fi
    rm -f "${zip}"
    printf '%s' "${want}" > "${DEST}/${n}.sha256"
    echo "Doctor log ${n}: verified and extracted"
done
echo "Doctor logs present and verified: ${DEST}"
