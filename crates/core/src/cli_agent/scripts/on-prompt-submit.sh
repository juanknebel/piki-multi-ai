#!/bin/sh
# Claude Code UserPromptSubmit hook -> piki cli-agent `prompt_submit` event.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)
BODY=$(build_payload "$INPUT" "prompt_submit")
emit_osc "$BODY"
exit 0
