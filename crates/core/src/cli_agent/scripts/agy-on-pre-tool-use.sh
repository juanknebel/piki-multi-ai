#!/bin/sh
# Antigravity `PreToolUse` hook -> piki cli-agent `permission_request` or
# `tool_started`.
#
# agy fires PreToolUse before EVERY tool step whether or not the user will be
# asked to approve it, so the hook predicts the permission gate from the two
# on-disk signals agy documents in `settings.json`:
#
# * `toolPermission` — when set to a non-default mode, piki can't interpret
#   the mode's vocabulary, so it fails safe to `tool_started` (never amber
#   without evidence).
# * `permissions.allow` — entries like `command(cargo test)`; a `run_command`
#   whose command line starts with an entry is treated as pre-approved.
# * `trustedWorkspaces` — a tool running under a trusted workspace is treated
#   as pre-approved (a false amber there would fire on every tool step for
#   the users who trust their checkouts, which is worse than a missed one).
#
# Anything unknown — missing settings, an unparsable file, a non-command
# tool piki can't build a permission string for — fails safe to
# `tool_started`: the tab stays Running and the follow-up `PostToolUse`
# (`tool_complete`) corrects a mispredicted `WaitingPermission` as soon as
# the tool finishes.
#
# The hook is a pure observer: stdout is always `{}` and never overrides
# agy's own permission policy.
#
# stdout must always be a JSON object — `{}` means "no post-processing".
[ -z "$PIKI_CLI_AGENT" ] && { echo '{}'; exit 0; }
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || { echo '{}'; exit 0; }
. "$DIR/agy-payload.sh"

INPUT=$(cat)
TOOL=$(printf '%s' "$INPUT" | jq -r '.toolCall.name // empty' 2>/dev/null)
WS=$(printf '%s' "$INPUT" | jq -r '.workspacePaths[0] // empty' 2>/dev/null)
SETTINGS="$HOME/.gemini/antigravity-cli/settings.json"

# Default verdict: the tool started, no evidence of a prompt.
EVENT="tool_started"

if [ "$TOOL" = "run_command" ]; then
    CMD=$(printf '%s' "$INPUT" | jq -r '.toolCall.args.CommandLine // empty' 2>/dev/null)
    if [ -n "$CMD" ] && [ -f "$SETTINGS" ]; then
        MODE=$(jq -r '.toolPermission // empty' "$SETTINGS" 2>/dev/null)
        if [ -z "$MODE" ] || [ "$MODE" = "default" ]; then
            # An allow entry `command(cargo test)` covers `cargo test --lib`
            # (word-prefix on the inner command) but not `cargo testing`.
            ALLOWED=$(jq -r --arg c "$CMD" \
                '[.permissions.allow // []
                  | .[] | capture("^command\\((?<rule>.*)\\)$")?.rule as $rule
                  | select($c == $rule or ($c | startswith($rule + " ")))] | length' \
                "$SETTINGS" 2>/dev/null)
            TRUSTED=$(jq -r --arg w "$WS" \
                '[.trustedWorkspaces // [] | .[] as $e | select($w != "" and ($w == $e or ($w | startswith($e + "/"))))] | length' \
                "$SETTINGS" 2>/dev/null)
            if [ "$ALLOWED" = "0" ] && [ "$TRUSTED" = "0" ]; then
                EVENT="permission_request"
            fi
        fi
    fi
fi

if [ "$EVENT" = "permission_request" ]; then
    PREVIEW=$(printf '%s' "$INPUT" | jq -r '.toolCall.args.CommandLine // empty' 2>/dev/null | cut -c1-120)
    SUMMARY="Wants to run $TOOL"
    [ -n "$PREVIEW" ] && SUMMARY="$SUMMARY: $PREVIEW"
    BODY=$(build_payload "$INPUT" "$EVENT" --arg tool_name "$TOOL" --arg summary "$SUMMARY")
else
    BODY=$(build_payload "$INPUT" "$EVENT" --arg tool_name "$TOOL")
fi
emit_osc "$BODY"
echo '{}'
exit 0
