#!/bin/sh
# Claude Code PermissionRequest hook -> piki cli-agent `permission_request`.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // "unknown"' 2>/dev/null)
PREVIEW=$(printf '%s' "$INPUT" | jq -r '
    (.tool_input
     | if .command then .command
       elif .file_path then .file_path
       else (tostring | .[0:80]) end) // ""' 2>/dev/null)
SUMMARY="Wants to run $TOOL_NAME"
if [ -n "$PREVIEW" ]; then
    PREVIEW=$(printf '%s' "$PREVIEW" | cut -c1-120)
    SUMMARY="$SUMMARY: $PREVIEW"
fi
BODY=$(build_payload "$INPUT" "permission_request" \
    --arg tool_name "$TOOL_NAME" \
    --arg summary "$SUMMARY")
emit_osc "$BODY"
exit 0
