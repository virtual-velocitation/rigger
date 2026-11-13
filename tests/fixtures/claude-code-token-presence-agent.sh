#!/bin/sh
# A fixture "agent" for the long-lived token's no-leak proof: it reports only WHETHER each token
# variable reached it, never a value, so what a spawn writes and emits can be searched for the
# value a test put in rigger's environment. It reads the host's first stdin line, then ends with
# the result line the cli host reads.
IFS= read -r _first
echo "CLAUDE_CODE_OAUTH_TOKEN_SET=${CLAUDE_CODE_OAUTH_TOKEN:+yes}"
echo "RIGGER_CLAUDE_CODE_OAUTH_TOKEN_SET=${RIGGER_CLAUDE_CODE_OAUTH_TOKEN:+yes}"
echo '{"id":"final","pass":true}'
