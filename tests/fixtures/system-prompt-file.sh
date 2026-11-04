# Sourced, never run, by the stand-in `claude` fixtures beside it: defines `system_prompt_file`,
# which prints the path its arguments name after `--system-prompt-file` (the last such, as the
# real binary reads it) and prints nothing when none does. A fixture passes it its own launch's
# arguments, `system_prompt_file "$@"`, and sources this file from the directory it is checked in
# under, which a test that links the fixture elsewhere reaches through the link:
# `self=$(readlink "$0") || self=$0; . "$(dirname "$self")/system-prompt-file.sh"`.
system_prompt_file() {
    spf_path=""
    spf_next=0
    for spf_arg in "$@"; do
        if [ "$spf_next" = "1" ]; then
            spf_path="$spf_arg"
            spf_next=0
        fi
        if [ "$spf_arg" = "--system-prompt-file" ]; then
            spf_next=1
        fi
    done
    printf '%s' "$spf_path"
}
