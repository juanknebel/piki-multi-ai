#!/bin/sh
# Claude Code SessionStart hook -> piki cli-agent `session_start` event.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)
BODY=$(build_payload "$INPUT" "session_start")
emit_osc "$BODY"
exit 0
