#!/bin/bash -eux
set -o pipefail

# sudo apt install unrar

[[ $# -eq 0 ]] && echo "Usage: $0 (--keep-cbr | --dont-keep-cbr) *.cbr" && exit 1

KEEP=
case "$1" in
    --keep-cbr) KEEP=yes ;;
    --dont-keep-cbr) KEEP=no ;;
    *) exit 1
esac
shift

until [[ "${1:-}" = '' ]]; do
    f=$1; shift
    echo "$f"
    [[ ! -f "$f" ]] && echo "Non existing: skipping" && continue
    naked=${f%.cbr}
    [[ "$naked" = "$f" ]] && echo "Not a CBR" && continue
    [[ -f "$naked".cbz ]] && echo "CBZ already exists" && exit 2
    if [[ -d "$naked" ]] && ! rmdir "$naked" >/dev/null; then
        echo "$naked path is not empty" && exit 2
    fi
    mkdir "$naked"

    echo Extracting...
    unrar x "$f" "$naked"

    echo Compressing "$naked"
    zip -r "$naked".cbz "$naked"

    rm -rf "$naked"
    [[ "$KEEP" = no ]] && rm "$f"
done
