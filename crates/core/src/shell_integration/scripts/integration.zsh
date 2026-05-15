# piki-multi shell integration for zsh.
# Sourced by the bridge .zshrc that piki creates in $ZDOTDIR before chaining
# to the user's real ~/.zshrc.
#
# Emits OSC 133 (prompt/command markers + exit code) and OSC 7 (cwd) so piki's
# OSC parser can extract structured events from the PTY stream.

# Bail if not opted in (env var is set by piki when spawning shell tabs).
[[ -z "$PIKI_SHELL_INTEGRATION" ]] && return

__piki_osc_prompt_start() { printf '\e]133;A\a' }
__piki_osc_input_start()  { printf '\e]133;B\a' }
__piki_osc_cmd_start()    { printf '\e]133;C\a' }
__piki_osc_cmd_end()      { printf '\e]133;D;%s\a' "$?" }
__piki_osc_cwd()          { printf '\e]7;file://%s%s\a' "${HOST:-${HOSTNAME:-}}" "$PWD" }

# Hook into precmd (runs before each prompt) and preexec (runs before each
# command). zsh appends to these arrays cleanly, so user hooks are preserved.
autoload -Uz add-zsh-hook
add-zsh-hook precmd __piki_osc_cmd_end
add-zsh-hook precmd __piki_osc_cwd
add-zsh-hook precmd __piki_osc_prompt_start
add-zsh-hook precmd __piki_osc_input_start
add-zsh-hook preexec __piki_osc_cmd_start

# Emit cwd once at startup so piki gets the initial directory before any
# command runs.
__piki_osc_cwd
