# Sourced, never run, by the stand-in `claude` fixtures beside it: reads what a launch's arguments
# name, as the real binary reads them. A fixture passes it its own launch's arguments, `"$@"`, and
# sources this file from the directory it is checked in under, which a test that links the fixture
# elsewhere reaches through the link:
# `self=$(readlink "$0") || self=$0; . "$(dirname "$self")/launch-args.sh"`.

# The value after the last `$1` among the rest of the arguments; nothing when none names it.
launch_arg() {
    la_flag=$1
    shift
    la_value=""
    la_next=0
    for la_arg in "$@"; do
        if [ "$la_next" = "1" ]; then
            la_value="$la_arg"
            la_next=0
        fi
        if [ "$la_arg" = "$la_flag" ]; then
            la_next=1
        fi
    done
    printf '%s' "$la_value"
}

# The path the arguments name after `--system-prompt-file`; nothing when none does.
system_prompt_file() {
    launch_arg --system-prompt-file "$@"
}

# The session the launch runs as: the `--session-id` the host minted (a fresh launch and the fork
# of a relaunch), else the session `--resume` continues, else `$1`, for a fixture run with neither.
launched_session() {
    ls_fallback=$1
    shift
    ls_session=$(launch_arg --session-id "$@")
    [ -n "$ls_session" ] || ls_session=$(launch_arg --resume "$@")
    printf '%s' "${ls_session:-$ls_fallback}"
}

# Copies its input to its output with the `session_id` of each `system/init` line read as `$1`, so
# a replayed transcript shows the session the launch runs as. Every other line, and every line
# when `$1` is empty, passes unchanged.
as_launched() {
    if [ -n "$1" ]; then
        sed "/\"subtype\":\"init\"/s/\"session_id\":\"[^\"]*\"/\"session_id\":\"$1\"/"
    else
        cat
    fi
}
