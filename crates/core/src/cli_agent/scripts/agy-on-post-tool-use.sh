#!/bin/sh
# Antigravity `PostToolUse` hook -> piki cli-agent `tool_complete` event.
#
# Keeps the tab on Running while the agent grinds through tool steps, so a long
# tool chain can't be mistaken for a finished turn. `toolCall` is null on some
# steps (agy fires PostToolUse for non-tool steps too), hence the `// empty`.
#
# stdout must always be a JSON object — `{}` means "no post-processing".
[ -z "$PIKI_CLI_AGENT" ] && { echo '{}'; exit 0; }
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || { echo '{}'; exit 0; }
. "$DIR/agy-payload.sh"

INPUT=$(cat)
TOOL=$(printf '%s' "$INPUT" | jq -r '.toolCall.name // empty' 2>/dev/null)
BODY=$(build_payload "$INPUT" "tool_complete" --arg tool_name "$TOOL")
emit_osc "$BODY"
echo '{}'
exit 0
