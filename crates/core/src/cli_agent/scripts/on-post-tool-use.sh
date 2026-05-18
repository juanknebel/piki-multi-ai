#!/bin/sh
# Claude Code PostToolUse hook -> piki cli-agent `tool_complete` event.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
BODY=$(build_payload "$INPUT" "tool_complete" --arg tool_name "$TOOL_NAME")
emit_osc "$BODY"
exit 0
