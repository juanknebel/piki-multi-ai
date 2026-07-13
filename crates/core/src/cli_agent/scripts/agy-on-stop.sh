#!/bin/sh
# Antigravity `Stop` hook -> piki cli-agent `stop` event (turn complete).
#
# Sends truncated query/response previews plus the transcript path, matching
# the Claude bridge's payload so both agents render identically in the Agents
# pane. agy's Stop payload carries no message text, so the previews are pulled
# out of `transcriptPath` (JSONL of {source,type,content} steps: the user's turn
# is USER_EXPLICIT/USER_INPUT wrapped in <USER_REQUEST> tags, the agent's reply
# is the last MODEL/PLANNER_RESPONSE with prose in it).
#
# stdout must always be a JSON object. An absent/any-other `decision` lets the
# agent stop; only "continue" would force it back into the loop.
[ -z "$PIKI_CLI_AGENT" ] && { echo '{}'; exit 0; }
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || { echo '{}'; exit 0; }
. "$DIR/agy-payload.sh"

INPUT=$(cat)

TP=$(printf '%s' "$INPUT" | jq -r '.transcriptPath // empty' 2>/dev/null)
QUERY=""
RESPONSE=""
if [ -n "$TP" ] && [ -f "$TP" ]; then
    # Stop fires before the last step is fully flushed; small grace period.
    sleep 0.3
    QUERY=$(jq -rs '
        [ .[] | select(.source == "USER_EXPLICIT" and .type == "USER_INPUT")
          | .content // "" ]
        | last // ""
        | capture("<USER_REQUEST>\\s*(?<q>[\\s\\S]*?)\\s*</USER_REQUEST>").q // .' \
        "$TP" 2>/dev/null)
    # Tool-call steps leave commentary markers in PLANNER_RESPONSE; those are
    # not prose the user wants to read back, so they're filtered out.
    RESPONSE=$(jq -rs '
        [ .[] | select(.source == "MODEL" and .type == "PLANNER_RESPONSE")
          | .content // ""
          | select(. != "" and (contains("<|channel|>") | not)) ]
        | last // ""' "$TP" 2>/dev/null)
    QUERY=$(printf '%s' "$QUERY" | cut -c1-500)
    RESPONSE=$(printf '%s' "$RESPONSE" | cut -c1-500)
fi

BODY=$(build_payload "$INPUT" "stop" \
    --arg query "$QUERY" \
    --arg response "$RESPONSE" \
    --arg transcript_path "$TP")
emit_osc "$BODY"
echo '{}'
exit 0
