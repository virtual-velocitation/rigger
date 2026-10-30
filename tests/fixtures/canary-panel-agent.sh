#!/bin/sh
# A fixture "agent" for `rigger canary` over a review panel: it stands in for the real `claude`
# binary and appends what each spawn was handed to the file `$CANARY_WALK` names (a path the test
# owns), so a test can read back the walk the panel really took: one line per spawn, its id and
# the `--model` value on its argv (`none` with no flag), and under an adjudicator's line the
# numbered findings its prompt lists, in their order. A reviewer raises one finding about
# `<role>.rs` under its role token, so a corpus item anchored at that file is caught by that spawn
# alone; an adjudicator rejects once any finding reached it. Checked in, never written at test
# time, for the same reason as its sibling fixtures.
: "${CANARY_WALK:?the test names the walk file}"
model=""
next=0
for a in "$@"; do
    if [ "$next" = "1" ]; then
        model="$a"
        next=0
    fi
    if [ "$a" = "--model" ]; then
        next=1
    fi
done
echo "$RIGGER_SPAWN ${model:-none}" >> "$CANARY_WALK"
role=${RIGGER_SPAWN#canary:*:}
case "$role" in
    adjudicator:*)
        findings=$(sed -n '/^[0-9][0-9]*\. \[/p')
        if [ -z "$findings" ]; then
            echo '{"verdict":"approve"}'
        else
            printf '%s\n' "$findings" >> "$CANARY_WALK"
            echo '{"verdict":"reject"}'
        fi
        ;;
    *)
        cat > /dev/null
        echo "{\"type\":\"ReviewFinding\",\"data\":{\"summary\":\"$role\",\"about\":[\"$role.rs\"]}}"
        echo '{"id":"final","pass":true}'
        ;;
esac
