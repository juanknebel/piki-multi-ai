#!/bin/sh
# Antigravity `PreInvocation` hook -> piki cli-agent `prompt_submit` event.
#
# Fires right before the model is called, which is the closest thing agy has to
# Claude Code's UserPromptSubmit: it marks the tab as working again (piki maps
# `prompt_submit` to CliAgentStatus::Running).
#
# stdout must always be a JSON object — `{}` means "inject nothing".
[ -z "$PIKI_CLI_AGENT" ] && { echo '{}'; exit 0; }
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || { echo '{}'; exit 0; }
. "$DIR/agy-payload.sh"

INPUT=$(cat)
BODY=$(build_payload "$INPUT" "prompt_submit")
emit_osc "$BODY"
echo '{}'
exit 0
