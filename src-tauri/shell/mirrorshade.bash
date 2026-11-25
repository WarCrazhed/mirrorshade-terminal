# Mirrorshade Terminal bash integration (works with macOS's bash 3.2 and Homebrew bash 5).
# Loaded with `bash --rcfile <this> -i`, so we first run the usual startup
# files ourselves, then install the hooks:
#   OSC 133;A/B/C/D  prompt / input / output / exit code  (command blocks)
#   OSC 7            current directory
#   OSC 6973;<nonce>;cmd|path;<base64>  command text and PATH (nonce-protected)

# --- emulate a login shell's startup (bash ignores --rcfile for -l) ---
if [ -z "$__ms_startup_done" ]; then
  __ms_startup_done=1
  [ -r /etc/profile ] && . /etc/profile
  if [ -r "$HOME/.bash_profile" ]; then . "$HOME/.bash_profile"
  elif [ -r "$HOME/.bash_login" ]; then . "$HOME/.bash_login"
  elif [ -r "$HOME/.profile" ]; then . "$HOME/.profile"
  elif [ -r "$HOME/.bashrc" ]; then . "$HOME/.bashrc"
  fi
fi

case "$-" in *i*) ;; *) return 0 ;; esac
[ -n "$__ms_loaded" ] && return 0
__ms_loaded=1

__ms_nonce="$MIRRORSHADE_NONCE"
__ms_pfile="$MIRRORSHADE_PROMPT_FILE"
__ms_theme="$MIRRORSHADE_PROMPT_THEME"
unset MIRRORSHADE_NONCE MIRRORSHADE_PROMPT_FILE MIRRORSHADE_PROMPT_THEME   # keep them out of child processes
__ms_running=""
__ms_path_sent=""
__ms_t0=""
__ms_grad=0
__ms_hex=()
__ms_active=""
__ms_user_ps1="$PS1"
__ms_ps=""

__ms_b64() { printf '%s' "$1" | /usr/bin/base64 | tr -d '\n'; }

# DEBUG trap: runs before every simple command. Only the first one after a
# prompt counts as "command started".
__ms_preexec() {
  [ -n "$COMP_LINE" ] && return
  [ -n "$__ms_in_pc" ] && return        # commands run by PROMPT_COMMAND
  [ -n "$__ms_running" ] && return
  case "$BASH_COMMAND" in __ms_*) return ;; esac
  __ms_running=1
  __ms_t0=$SECONDS
  local h
  h=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null)
  h="${h#"${h%%[![:space:]]*}"}"   # trim leading spaces
  h="${h#*[[:space:]]}"            # drop the history number
  h="${h#"${h%%[![:space:]]*}"}"
  [ -z "$h" ] && h="$BASH_COMMAND"
  printf '\e]6973;%s;cmd;%s\a' "$__ms_nonce" "$(__ms_b64 "$h")"
  printf '\e]133;C\a'
}

__ms_load_state() {
  [ -r "$__ms_pfile" ] || return
  local k v line
  while IFS= read -r line || [ -n "$line" ]; do
    k="${line%%=*}"; v="${line#*=}"
    case "$k" in
      theme) case "$v" in two-line|minimal|blocks) __ms_theme="$v" ;; none) __ms_theme="" ;; esac ;;
      gradient) case "$v" in 0|1) __ms_grad="$v" ;; esac ;;
      colors)
        local IFS=,
        local -a cs
        cs=($v)
        if [ "${#cs[@]}" -eq 16 ]; then
          local c ok=1
          for c in "${cs[@]}"; do
            case "$c" in \#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]) ;; *) ok=0 ;; esac
          done
          [ $ok = 1 ] && __ms_hex=("${cs[@]}")
        fi
        ;;
    esac
  done < "$__ms_pfile"
}

# Git branch straight from .git (no `git` process: fast and safe in untrusted repos).
__ms_git_branch() {
  REPLY=""
  local d="$PWD" head="" gd
  while :; do
    if [ -f "$d/.git/HEAD" ]; then read -r head < "$d/.git/HEAD"; break
    elif [ -f "$d/.git" ]; then
      read -r gd < "$d/.git"; gd="${gd#gitdir: }"
      case "$gd" in /*) ;; *) gd="$d/$gd" ;; esac
      [ -f "$gd/HEAD" ] && read -r head < "$gd/HEAD"
      break
    fi
    [ -z "$d" ] || [ "$d" = "/" ] && break
    d="${d%/*}"; [ -z "$d" ] && d="/"
  done
  [ -z "$head" ] && return
  case "$head" in ref:*) REPLY="${head#ref: refs/heads/}" ;; *) REPLY="${head:0:7}" ;; esac
}

# "#rrggbb" "#rrggbb" t(0..1000) -> REPLY "r;g;b"
__ms_mix() {
  local a="$1" b="$2" t=$3
  local r=$(( 16#${a:1:2} + (16#${b:1:2} - 16#${a:1:2}) * t / 1000 ))
  local g=$(( 16#${a:3:2} + (16#${b:3:2} - 16#${a:3:2}) * t / 1000 ))
  local bl=$(( 16#${a:5:2} + (16#${b:5:2} - 16#${a:5:2}) * t / 1000 ))
  REPLY="$r;$g;$bl"
}
__ms_hexmix() { __ms_mix "$@"; local IFS=';'; set -- $REPLY; printf -v REPLY '#%02x%02x%02x' "$1" "$2" "$3"; }

# The prompt is built into $__ms_ps and PS1='${__ms_ps}': bash expands the
# variable once and never re-expands its value, so a folder or branch named
# '$(cmd)' is shown literally. \001/\002 mark non-printing bytes for readline.
__ms_build_prompt() {
  local ret=$1 O=$'\001' C=$'\002' E=$'\e' SEP=$'\xee\x82\xb0'
  local took="" branch venv dir
  if [ -n "$__ms_t0" ]; then
    local secs=$(( SECONDS - __ms_t0 ))
    __ms_t0=""
    if [ $secs -ge 60 ]; then took="$((secs / 60))m$((secs % 60))s"; elif [ $secs -ge 2 ]; then took="${secs}s"; fi
  fi
  __ms_git_branch; branch="$REPLY"
  branch="${branch//[^A-Za-z0-9._\/+-]/?}"
  venv=""; [ -n "$VIRTUAL_ENV" ] && venv="${VIRTUAL_ENV##*/}"
  dir="$PWD"
  case "$dir" in "$HOME") dir="~" ;; "$HOME"/*) dir="~${dir#"$HOME"}" ;; esac
  # keep the last 3 components
  local rest="$dir" n=0
  while [ "$rest" != "${rest#*/}" ]; do rest="${rest#*/}"; n=$((n + 1)); done
  if [ $n -gt 3 ]; then
    local tail="${dir#*/}" i=$n
    while [ $i -gt 3 ]; do tail="${tail#*/}"; i=$((i - 1)); done
    dir="…/$tail"
  fi
  local fg="${O}${E}[39m${C}" bold="${O}${E}[1m${C}" reset="${O}${E}[0m${C}"
  local arrow="❯"
  [ "$ret" != 0 ] && local acol=1 || local acol=2

  case "$__ms_theme" in
    two-line)
      local seg=""
      [ -n "$branch" ] && seg+=" ${O}${E}[38;5;8m${C}on ${O}${E}[38;5;5m${C}⎇ ${branch}"
      [ -n "$venv" ] && seg+=" ${O}${E}[38;5;8m${C}via ${O}${E}[38;5;3m${C}◆ ${venv}"
      [ "$ret" != 0 ] && seg+=" ${O}${E}[38;5;1m${C}✗ $ret"
      [ -n "$took" ] && seg+=" ${O}${E}[38;5;8m${C}⏱ $took"
      __ms_ps=$'\n'"${O}${E}[38;5;8m${C}╭─ ${O}${E}[38;5;6m${C}${USER}${O}${E}[38;5;8m${C}@${HOSTNAME%%.*} ${bold}${O}${E}[38;5;4m${C}${dir}${reset}${seg}${reset}"$'\n'"${O}${E}[38;5;8m${C}╰─${O}${E}[38;5;${acol}m${C}${arrow}${reset} "
      ;;
    minimal)
      local seg=""
      [ -n "$branch" ] && seg+=" ${O}${E}[38;5;8m${C}on ${O}${E}[38;5;5m${C}⎇ ${branch}"
      [ -n "$took" ] && seg+=" ${O}${E}[38;5;8m${C}⏱ $took"
      __ms_ps="${bold}${O}${E}[38;5;4m${C}${dir}${reset}${seg} ${O}${E}[38;5;${acol}m${C}${arrow}${reset} "
      ;;
    blocks)
      local -a sb sf st
      local status_txt="" status_fg=6 jobs
      jobs=$(jobs -p | wc -l | tr -d ' ')
      [ "$ret" != 0 ] && { status_txt+="✘ $ret "; status_fg=1; }
      [ "$EUID" = 0 ] && { status_txt+="root "; [ $status_fg = 6 ] && status_fg=3; }
      [ "${jobs:-0}" -gt 0 ] && status_txt+="⚙ $jobs "
      [ -n "$status_txt" ] && { sb+=(0); sf+=($status_fg); st+=("${status_txt% }"); }
      [ -n "$SSH_CONNECTION" ] && { sb+=(0); sf+=(7); st+=("${USER}@${HOSTNAME%%.*}"); }
      sb+=(4); sf+=(0); st+=("$dir")
      [ -n "$branch" ] && { sb+=(2); sf+=(0); st+=("⎇ ${branch}"); }
      [ -n "$venv" ] && { sb+=(3); sf+=(0); st+=("◆ ${venv}"); }
      [ -n "$took" ] && { sb+=(8); sf+=(15); st+=("⏱ $took"); }
      local out="" i nseg=${#st[@]}
      if [ "$__ms_grad" = 1 ] && [ ${#__ms_hex[@]} -eq 16 ]; then
        local -a from to
        for (( i = 0; i < nseg; i++ )); do
          from[i]="${__ms_hex[${sb[i]}]}"
          if (( i < nseg - 1 )); then __ms_hexmix "${from[i]}" "${__ms_hex[${sb[i+1]}]}" 700
          else __ms_hexmix "${from[i]}" "#ffffff" 300; fi
          to[i]="$REPLY"
        done
        local text j len
        for (( i = 0; i < nseg; i++ )); do
          if (( i > 0 )); then
            __ms_mix "${from[i]}" "${from[i]}" 0; local bgc="$REPLY"
            __ms_mix "${to[i-1]}" "${to[i-1]}" 0; local fgc="$REPLY"
            out+="${O}${E}[48;2;${bgc}m${E}[38;2;${fgc}m${C}${SEP}"
          fi
          out+="${O}${E}[38;5;${sf[i]}m${C}"
          text=" ${st[i]} "; len=${#text}
          for (( j = 0; j < len; j++ )); do
            __ms_mix "${from[i]}" "${to[i]}" $(( len > 1 ? j * 1000 / (len - 1) : 0 ))
            out+="${O}${E}[48;2;${REPLY}m${C}${text:j:1}"
          done
        done
        __ms_mix "${to[nseg-1]}" "${to[nseg-1]}" 0
        __ms_ps="${out}${O}${E}[49m${E}[38;2;${REPLY}m${C}${SEP}${reset} "
      else
        for (( i = 0; i < nseg; i++ )); do
          (( i > 0 )) && out+="${O}${E}[48;5;${sb[i]}m${E}[38;5;${sb[i-1]}m${C}${SEP}"
          out+="${O}${E}[48;5;${sb[i]}m${E}[38;5;${sf[i]}m${C} ${st[i]} "
        done
        __ms_ps="${out}${O}${E}[49m${E}[38;5;${sb[nseg-1]}m${C}${SEP}${reset} "
      fi
      ;;
  esac
  __ms_ps+="${O}${E}]133;B"$'\a'"${C}"
}

# First in PROMPT_COMMAND: grab the exit status before anything clobbers it.
__ms_save_status() { __ms_ret=$?; __ms_in_pc=1; }

# Last in PROMPT_COMMAND, so prompt frameworks that rewrite PS1 run first.
__ms_precmd() {
  local ret=${__ms_ret:-0}
  if [ -n "$__ms_running" ]; then printf '\e]133;D;%s\a' "$ret"; fi
  __ms_running=""
  printf '\e]7;file://%s%s\a' "${HOSTNAME}" "${PWD// /%20}"
  if [ "$__ms_path_sent" != "$PATH" ]; then
    __ms_path_sent="$PATH"
    printf '\e]6973;%s;path;%s\a' "$__ms_nonce" "$(__ms_b64 "$PATH")"
  fi
  printf '\e]133;A\a'
  __ms_load_state
  if [ -n "$__ms_theme" ]; then
    __ms_build_prompt "$ret"
    PS1='${__ms_ps}'
    __ms_active=1
  else
    if [ -n "$__ms_active" ]; then PS1="$__ms_user_ps1"; __ms_active=""; fi
    case "$PS1" in *'133;B'*) ;; *) PS1="$PS1"'\[\e]133;B\a\]' ;; esac
  fi
  __ms_in_pc=""
  return $ret
}

shopt -s promptvars 2>/dev/null
# Ignore the DEBUG trap until the first prompt (this file's own setup
# commands would otherwise look like a user command); precmd clears it.
__ms_in_pc=1
trap '__ms_preexec' DEBUG
case ";${PROMPT_COMMAND};" in
  *";__ms_precmd;"*) ;;
  *) PROMPT_COMMAND="__ms_save_status${PROMPT_COMMAND:+;$PROMPT_COMMAND};__ms_precmd" ;;
esac
