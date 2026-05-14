# piki-multi shell integration for fish.
# Loaded via `fish -C 'source <path>'` injected by piki when spawning a fish
# tab. User's config.fish has already been sourced by the time -C runs, so
# our event handlers register on top of any user setup (event handlers stack
# in fish — they don't clobber each other).
#
# Emits OSC 133 (prompt/command markers + exit code) and OSC 7 (cwd) so piki's
# OSC parser can extract structured events from the PTY stream.

# Bail if not opted in (env var is set by piki when spawning shell tabs).
# Wrapping in `if` instead of `return`/`exit` — `exit` from a sourced file
# kills the whole interactive shell in fish.
if test -n "$PIKI_SHELL_INTEGRATION"
    function __piki_osc_prompt_start; printf '\e]133;A\a'; end
    function __piki_osc_input_start;  printf '\e]133;B\a'; end
    function __piki_osc_cmd_start;    printf '\e]133;C\a'; end
    function __piki_osc_cmd_end;      printf '\e]133;D;%s\a' $argv[1]; end
    function __piki_osc_cwd;          printf '\e]7;file://%s%s\a' (hostname 2>/dev/null; or echo -n) "$PWD"; end

    # fish event handlers — additive, named so they don't conflict with
    # user-defined functions of the same name.
    function __piki_on_postexec --on-event fish_postexec
        __piki_osc_cmd_end $status
    end
    function __piki_on_prompt --on-event fish_prompt
        __piki_osc_cwd
        __piki_osc_prompt_start
        __piki_osc_input_start
    end
    function __piki_on_preexec --on-event fish_preexec
        __piki_osc_cmd_start
    end

    # Emit cwd once at startup so piki gets the initial directory before
    # any command runs (matches the bash/zsh integration).
    __piki_osc_cwd
end
