#!/bin/bash -eu
set -o pipefail

# sudo apt install unrar

[[ $# -eq 0 ]] && echo "Usage: $0 (--keep | --dont-keep) *.cbr" && exit 1

KEEP=
case "$1" in
    --keep) KEEP=yes ;;
    --dont-keep) KEEP=no ;;
    *) exit 1
esac
shift

until [[ "${1:-}" = '' ]]; do
    f=$1; shift
    [[ ! -f "$f" ]] && "Not a CBR: $f" && exit 2

    naked=${f%%.*}
    [[ -f "$naked".cbz ]] && continue
    if [[ -d "$naked" ]] && ! rmdir "$naked" >/dev/null; then
        echo "$naked path is not empty" && exit 2
    fi
    mkdir "$naked"

    echo Extracting "$f"
    unrar x "$f" "$naked"

    echo Compressing "$naked"
    zip -r "$naked".cbz "$naked"

    rm -r "$naked"
    [[ "$KEEP" = no ]] && rm "$f"
done
