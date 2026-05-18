# piki-multi cli-agent payload helper. Sourced by the on-*.sh hook scripts.
#
# Builds the {v,agent,event,session_id,cwd,project,...} JSON payload and
# emits it in-band as an OSC 777 sequence to the controlling TTY, where
# piki's OscParser sniffs it out of the PTY byte stream.
#
# Requires `jq`. The hook scripts bail (exit 0) before sourcing this when
# PIKI_CLI_AGENT is unset, so this is inert outside piki.

# Protocol version negotiated as min(script, piki). piki sets PIKI_CLI_AGENT_V.
PIKI_CLI_AGENT_SCRIPT_V=1

_negotiate_v() {
    warp_v="${PIKI_CLI_AGENT_V:-1}"
    if [ "$warp_v" -lt "$PIKI_CLI_AGENT_SCRIPT_V" ] 2>/dev/null; then
        printf '%s' "$warp_v"
    else
        printf '%s' "$PIKI_CLI_AGENT_SCRIPT_V"
    fi
}

# build_payload <hook-stdin-json> <event> [extra jq --arg/--argjson pairs...]
build_payload() {
    _input="$1"
    _event="$2"
    shift 2

    _v=$(_negotiate_v)
    _session_id=$(printf '%s' "$_input" | jq -r '.session_id // empty' 2>/dev/null)
    _cwd=$(printf '%s' "$_input" | jq -r '.cwd // empty' 2>/dev/null)
    _project=""
    [ -n "$_cwd" ] && _project=$(basename "$_cwd" 2>/dev/null)

    jq -nc \
        --argjson v "$_v" \
        --arg agent "claude" \
        --arg event "$_event" \
        --arg session_id "$_session_id" \
        --arg cwd "$_cwd" \
        --arg project "$_project" \
        "$@" \
        '{v:$v,agent:$agent,event:$event,session_id:$session_id,cwd:$cwd,project:$project} + $ARGS.named'
}

# emit_osc <json-body>
#
# Prefer the out-of-band per-tab FIFO (env var set by piki, file held open
# O_RDWR on piki's side so this never blocks/EOFs). Claude Code spawns hooks
# setsid-detached with no controlling terminal, so the /dev/tty OSC write below
# always fails there — the FIFO path is the one that actually delivers. The
# OSC 777 write stays as a graceful-degradation fallback for environments
# where the FIFO isn't available.
emit_osc() {
    if [ -n "$PIKI_CLI_AGENT_SOCK" ] && [ -p "$PIKI_CLI_AGENT_SOCK" ]; then
        printf '%s\n' "$1" > "$PIKI_CLI_AGENT_SOCK" 2>/dev/null || true
    else
        printf '\033]777;notify;%s;%s\007' \
            "${PIKI_CLI_AGENT_TARGET:-piki://cli-agent}" "$1" \
            > /dev/tty 2>/dev/null || true
    fi
}
