# piki-multi cli-agent payload helper for the Antigravity CLI (`agy`).
# Sourced by the agy-on-*.sh hook scripts.
#
# Antigravity's hook payloads differ from Claude Code's: keys are camelCase
# (protojson), the session id is `conversationId`, and the workspace arrives
# as `workspacePaths[]`. This maps them onto piki's {v,agent,event,session_id,
# cwd,project,...} protocol and ships the result down the same per-tab FIFO.
#
# Every agy hook MUST print a JSON object on stdout or the agent loop treats
# the handler as failed — the callers do that unconditionally, including on
# the inert path (PIKI_CLI_AGENT unset).
#
# Requires `jq`.

PIKI_CLI_AGENT_SCRIPT_V=1

_negotiate_v() {
    piki_v="${PIKI_CLI_AGENT_V:-1}"
    if [ "$piki_v" -lt "$PIKI_CLI_AGENT_SCRIPT_V" ] 2>/dev/null; then
        printf '%s' "$piki_v"
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
    _session_id=$(printf '%s' "$_input" | jq -r '.conversationId // empty' 2>/dev/null)
    _cwd=$(printf '%s' "$_input" | jq -r '.workspacePaths[0] // empty' 2>/dev/null)
    _project=""
    [ -n "$_cwd" ] && _project=$(basename "$_cwd" 2>/dev/null)

    jq -nc \
        --argjson v "$_v" \
        --arg agent "antigravity" \
        --arg event "$_event" \
        --arg session_id "$_session_id" \
        --arg cwd "$_cwd" \
        --arg project "$_project" \
        "$@" \
        '{v:$v,agent:$agent,event:$event,session_id:$session_id,cwd:$cwd,project:$project} + $ARGS.named'
}

# emit_osc <json-body>
#
# Same transport as the Claude bridge: prefer the per-tab FIFO piki advertises
# via PIKI_CLI_AGENT_SOCK (held open O_RDWR on piki's side, so this never
# blocks or EOFs), and fall back to an in-band OSC 777 write to the controlling
# TTY. agy runs hooks as children of the PTY process, so unlike Claude Code the
# /dev/tty fallback does work here — the FIFO stays preferred because it can't
# be corrupted by concurrent terminal output.
emit_osc() {
    if [ -n "$PIKI_CLI_AGENT_SOCK" ] && [ -p "$PIKI_CLI_AGENT_SOCK" ]; then
        printf '%s\n' "$1" > "$PIKI_CLI_AGENT_SOCK" 2>/dev/null || true
    else
        printf '\033]777;notify;%s;%s\007' \
            "${PIKI_CLI_AGENT_TARGET:-piki://cli-agent}" "$1" \
            > /dev/tty 2>/dev/null || true
    fi
}
