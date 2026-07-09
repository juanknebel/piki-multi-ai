# piki-multi shell integration for bash.
# Sourced by the bridge --rcfile that piki passes to bash before it reads
# ~/.bashrc (we source ~/.bashrc afterwards so user dotfiles still win for
# variables they explicitly override).
#
# Emits OSC 133 (prompt/command markers + exit code) and OSC 7 (cwd) so piki's
# OSC parser can extract structured events from the PTY stream.

# Bail if not opted in (env var is set by piki when spawning shell tabs).
[ -z "$PIKI_SHELL_INTEGRATION" ] && return

__piki_osc_prompt_start() { printf '\e]133;A\a'; }
__piki_osc_input_start()  { printf '\e]133;B\a'; }
__piki_osc_cmd_start()    { printf '\e]133;C\a'; }
__piki_osc_cmd_end()      { printf '\e]133;D;%s\a' "$?"; }
__piki_osc_cwd()          { printf '\e]7;file://%s%s\a' "${HOSTNAME:-}" "$PWD"; }

# PROMPT_COMMAND is bash's hook that runs before each interactive prompt.
# We append to it (instead of overwriting) so user setup keeps working.
__piki_pre_prompt() {
    local last_status=$?
    __piki_osc_cmd_end_with_status() { printf '\e]133;D;%s\a' "$last_status"; }
    __piki_osc_cmd_end_with_status
    __piki_osc_cwd
    __piki_osc_prompt_start
    __piki_osc_input_start
}
PROMPT_COMMAND="__piki_pre_prompt${PROMPT_COMMAND:+; $PROMPT_COMMAND}"

# DEBUG trap fires before every simple command. We use it to mark command
# start. Skip if we're inside the prompt hook itself (BASH_COMMAND would be
# `__piki_pre_prompt`).
__piki_debug_trap() {
    case "$BASH_COMMAND" in
        __piki_pre_prompt|__piki_*) return ;;
    esac
    __piki_osc_cmd_start
}
trap '__piki_debug_trap' DEBUG

# Wrap a manually-typed `claude` so it loads piki's hook settings and reports
# its status to the Agents pane (env PIKI_CLI_AGENT_SOCK et al. are already in
# this shell's environment). Skipped if the user passes their own --settings.
if [ -n "$PIKI_CLAUDE_HOOK_SETTINGS" ]; then
    claude() {
        case " $* " in
            *" --settings "*|*" --settings="*) command claude "$@" ;;
            *) command claude --settings "$PIKI_CLAUDE_HOOK_SETTINGS" "$@" ;;
        esac
    }
fi

# Emit cwd once at startup.
__piki_osc_cwd
