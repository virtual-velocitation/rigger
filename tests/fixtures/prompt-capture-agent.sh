#!/bin/sh
# A fixture "agent" for prompt delivery: it stands in for the real `claude` binary and copies
# what it was handed into the directory `$PROMPT_CAPTURE_DIR` names (a path the test owns,
# outside the spawn's scratch) - the whole of its stdin to `stdin`, and the file its
# `--system-prompt-file` argument names to `system-prompt` (and that path itself to
# `system-prompt-path`) - so a test can read back that a
# prompt larger than the kernel's per-argument limit arrived intact. Checked in, never written
# at test time, for the same reason as its sibling fixtures.
: "${PROMPT_CAPTURE_DIR:?the test names the capture directory}"
sp=""
next=0
for a in "$@"; do
    if [ "$next" = "1" ]; then
        sp="$a"
        next=0
    fi
    if [ "$a" = "--system-prompt-file" ]; then
        next=1
    fi
done
cat > "$PROMPT_CAPTURE_DIR/stdin"
if [ -n "$sp" ]; then
    cat "$sp" > "$PROMPT_CAPTURE_DIR/system-prompt"
    printf '%s' "$sp" > "$PROMPT_CAPTURE_DIR/system-prompt-path"
fi
echo '{"id":"final","pass":true}'
