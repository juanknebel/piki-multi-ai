#!/bin/sh
# Claude Code Stop hook -> piki cli-agent `stop` event (task complete).
#
# Sends truncated query/response previews + the transcript path. The full
# text stays in the transcript file (read lazily by the UI) so the OSC
# payload stays small and we dodge the Stop-before-flush race.
[ -z "$PIKI_CLI_AGENT" ] && exit 0
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
. "$DIR/build-payload.sh"
INPUT=$(cat)

# Don't double-fire when a Stop hook is already active.
ACTIVE=$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null)
[ "$ACTIVE" = "true" ] && exit 0

TP=$(printf '%s' "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null)
QUERY=""
RESPONSE=""
if [ -n "$TP" ] && [ -f "$TP" ]; then
    # Stop fires before the turn is fully flushed; small grace period.
    sleep 0.3
    QUERY=$(jq -rs '
        [ .[] | select(.type == "user")
          | if (.message.content | type) == "string" then .
            elif ([.message.content[]? | select(.type == "text")] | length) > 0 then .
            else empty end ]
        | last
        | if (.message.content | type) == "array"
          then [.message.content[] | select(.type == "text") | .text] | join(" ")
          else (.message.content // "") end' "$TP" 2>/dev/null)
    RESPONSE=$(jq -rs '
        [ .[] | select(.type == "assistant" and .message.content) ]
        | last
        | [.message.content[] | select(.type == "text") | .text] | join(" ")' \
        "$TP" 2>/dev/null)
    QUERY=$(printf '%s' "$QUERY" | cut -c1-500)
    RESPONSE=$(printf '%s' "$RESPONSE" | cut -c1-500)
fi

BODY=$(build_payload "$INPUT" "stop" \
    --arg query "$QUERY" \
    --arg response "$RESPONSE" \
    --arg transcript_path "$TP")
emit_osc "$BODY"
exit 0
