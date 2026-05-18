#!/bin/sh
# Claude Code Notification hook (matcher: idle_prompt) -> piki cli-agent
# `notification` event. Fires when Claude has been idle and wants input.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)
BODY=$(build_payload "$INPUT" "notification" --arg kind "idle_prompt")
emit_osc "$BODY"
exit 0
