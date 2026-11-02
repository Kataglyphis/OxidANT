#!/usr/bin/env bash
# Table-driven option parsing for local scripts; the hub parser accepts an empty value, which these scripts refuse.

cli_args_unknown() {
    printf 'unknown argument: %s\n' "$1" >&2
    exit 2
}

# cli_args_parse MISSING_WORD HELP_FN UNKNOWN_FN SPEC... -- ARGS...; SPEC is --flag=VAR or --flag=VAR=VALUE; UNKNOWN_FN must not return.
cli_args_parse() {
    local _ca_word="$1" _ca_help="$2" _ca_unknown="$3" _ca_target
    local -A _ca_values=() _ca_switches=()
    shift 3
    while [ "$1" != -- ]; do
        _ca_target="${1#*=}"
        case "${_ca_target}" in
            *=*) _ca_switches["${1%%=*}"]="${_ca_target}" ;;
            *) _ca_values["${1%%=*}"]="${_ca_target}" ;;
        esac
        shift
    done
    shift
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -h|--help) "${_ca_help}"; exit 0 ;;
            '') "${_ca_unknown}" "$1" ;;
        esac
        if [ -n "${_ca_values[$1]:-}" ]; then
            if [ -n "${_ca_word}" ]; then : "${2:?$1 needs ${_ca_word}}"; else : "${2:?}"; fi
            printf -v "${_ca_values[$1]}" '%s' "$2"
            shift 2
        elif [ -n "${_ca_switches[$1]:-}" ]; then
            printf -v "${_ca_switches[$1]%%=*}" '%s' "${_ca_switches[$1]#*=}"
            shift
        else
            "${_ca_unknown}" "$1"
        fi
    done
}
