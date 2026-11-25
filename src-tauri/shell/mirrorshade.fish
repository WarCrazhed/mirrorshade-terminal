# Mirrorshade Terminal fish integration — loaded with `fish -l --init-command "source <this>"`,
# i.e. after the user's config.fish. Emits:
#   OSC 133;A/B/C/D  prompt / input / output / exit code  (command blocks)
#   OSC 7            current directory
#   OSC 6973;<nonce>;cmd|path;<base64>  command text and PATH (nonce-protected)
# fish keeps its own prompt (prompt themes are zsh/bash only).

status is-interactive; or exit
set -q __ms_loaded; and exit
set -g __ms_loaded 1

set -g __ms_nonce "$MIRRORSHADE_NONCE"
set -e MIRRORSHADE_NONCE
set -e MIRRORSHADE_PROMPT_FILE
set -e MIRRORSHADE_PROMPT_THEME
set -g __ms_running 0
set -g __ms_path_sent ""

function __ms_b64
    printf '%s' "$argv[1]" | /usr/bin/base64 | tr -d '\n'
end

function __ms_preexec --on-event fish_preexec
    set -g __ms_running 1
    printf '\e]6973;%s;cmd;%s\a' "$__ms_nonce" (__ms_b64 "$argv[1]")
    printf '\e]133;C\a'
end

function __ms_postexec --on-event fish_postexec
    set -l code $status
    if test "$__ms_running" = 1
        printf '\e]133;D;%s\a' $code
    end
    set -g __ms_running 0
end

function __ms_prompt_start --on-event fish_prompt
    printf '\e]7;file://%s%s\a' "$hostname" (string replace -a ' ' '%20' -- "$PWD")
    set -l p (string join : -- $PATH)
    if test "$p" != "$__ms_path_sent"
        set -g __ms_path_sent "$p"
        printf '\e]6973;%s;path;%s\a' "$__ms_nonce" (__ms_b64 "$p")
    end
    printf '\e]133;A\a'
end

# Mark the end of the prompt (start of input) after the user's own prompt.
if functions -q fish_prompt
    functions -c fish_prompt __ms_user_prompt
    function fish_prompt
        __ms_user_prompt
        printf '\e]133;B\a'
    end
end
