#!/usr/bin/env bash
# Check that every relative link and image in the repository's markdown files points at a file or
# folder that exists. (External http(s) links and #anchors are not checked.)
#
# Usage: scripts/check_docs.sh        Exit status 1 if any link is broken.
set -u
cd "$(dirname "$0")/.."

status=0
# Tracked and untracked-but-not-ignored markdown files; third-party ROM bundles and build output are skipped.
files=$(git ls-files --cached --others --exclude-standard -- '*.md')
for f in $files; do
    dir=$(dirname "$f")
    # Inline links and images: [text](target) -- the target ends at ")" or at whitespace (a title).
    grep -oE '\]\([^)[:space:]]+' "$f" | sed 's/^](//' | while read -r target; do
        case "$target" in
            http://*|https://*|mailto:*|\#*) continue ;;
        esac
        path="${target%%#*}"          # drop an #anchor
        [ -z "$path" ] && continue
        if [ ! -e "$dir/$path" ]; then
            echo "BROKEN: $f -> $target"
            echo broken > .check_docs_failed
        fi
    done
done

if [ -e .check_docs_failed ]; then
    rm -f .check_docs_failed
    echo "some markdown links are broken"
    exit 1
fi
echo "all relative markdown links resolve ($(echo "$files" | wc -w) files checked)"
