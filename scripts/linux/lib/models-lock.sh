#!/usr/bin/env bash
# The file names a models.lock pins, so packaging repeats none of them; not covered: the files themselves, which verify_locked_files.py checks.

# models_lock_files LOCK: every pinned file name, one per line; a missing lock fails.
models_lock_files() {
    awk '!/^[[:space:]]*(#|$)/ && NF >= 3 { print $2 }' "$1"
}

# models_lock_file LOCK SUFFIX: the one pinned file ending in SUFFIX; none or several fail.
models_lock_file() {
    local lock="$1" suffix="$2" names name found="" count=0
    names="$(models_lock_files "$lock")" || return 1
    while IFS= read -r name; do
        case "$name" in
            *"$suffix") found="$name"; count=$((count + 1)) ;;
        esac
    done <<< "$names"
    if [ "$count" -ne 1 ]; then
        printf '%s pins %s file(s) ending in %s, not one\n' "$lock" "$count" "$suffix" >&2
        return 1
    fi
    printf '%s\n' "$found"
}
