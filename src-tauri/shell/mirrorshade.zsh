# Mirrorshade Terminal hooks: semantic prompt marks (OSC 133) + cwd (OSC 7) + command text.
#   133;A prompt start · 133;B prompt end / input start · 133;C output start · 133;D;<exit> done
#   6973;<nonce>;cmd;<base64>   command line as typed (nonce stops programs from spoofing it)
#   6973;<nonce>;path;<base64>  PATH, so autocomplete can list executables
[[ -n "$__ms_loaded" ]] && return
typeset -g __ms_loaded=1
typeset -g __ms_nonce="$MIRRORSHADE_NONCE"
unset MIRRORSHADE_NONCE   # keep it out of child processes' environment
typeset -g __ms_theme="$MIRRORSHADE_PROMPT_THEME"
unset MIRRORSHADE_PROMPT_THEME MIRRORSHADE_PROMPT_FILE
# The app rewrites this file when you change the prompt style, gradient or
# color theme; it's re-read before every prompt so changes apply live.
typeset -g __ms_pfile="${MIRRORSHADE_INTEGRATION_DIR:h}/prompt-state"
typeset -g __ms_grad=0
typeset -ga __ms_hex=()
typeset -g __ms_active=0
typeset -g __ms_user_prompt="$PROMPT" __ms_user_rprompt="$RPROMPT"
typeset -g __ms_running=""
typeset -g __ms_path_sent=""

__ms_b64() { printf '%s' "$1" | /usr/bin/base64 | tr -d '\n' }

__ms_precmd() {
  local ret=$?
  __ms_load_state
  if [[ -n "$__ms_theme" ]]; then
    __ms_prompt "$ret"
    __ms_active=1
  elif (( __ms_active )); then
    # Switched back to "your own prompt": restore what .zshrc had set.
    PROMPT="$__ms_user_prompt"; RPROMPT="$__ms_user_rprompt"
    __ms_active=0
  fi
  if [[ -n "$__ms_running" ]]; then
    printf '\e]133;D;%s\a' "$ret"
  fi
  __ms_running=""
  printf '\e]7;file://%s%s\a' "${HOST}" "${PWD// /%20}"
  if [[ "$__ms_path_sent" != "$PATH" ]]; then
    __ms_path_sent="$PATH"
    printf '\e]6973;%s;path;%s\a' "$__ms_nonce" "$(__ms_b64 "$PATH")"
  fi
  printf '\e]133;A\a'
  # Mark end of prompt. Themes rebuild PS1 every precmd, so re-append each time.
  if [[ "$PS1" != *$'\e]133;B\a'* ]]; then
    PS1="$PS1%{"$'\e]133;B\a'"%}"
  fi
}

__ms_preexec() {
  __ms_running=1
  __ms_t0=$EPOCHREALTIME
  printf '\e]6973;%s;cmd;%s\a' "$__ms_nonce" "$(__ms_b64 "$1")"
  printf '\e]133;C\a'
}

# ---------------------------------------------------------------------------
# Prompt themes (MIRRORSHADE_PROMPT_THEME = two-line | minimal). Colors use ANSI indexes so
# they follow the terminal theme. The git branch is read straight from .git —
# no `git` process (fast, and safe in untrusted repos).
# ---------------------------------------------------------------------------
zmodload zsh/datetime 2>/dev/null
typeset -g __ms_t0=""

__ms_git_branch() {
  REPLY=""
  local d="$PWD" head="" gd
  while true; do
    if [[ -f "$d/.git/HEAD" ]]; then
      head="$(<"$d/.git/HEAD")"; break
    elif [[ -f "$d/.git" ]]; then
      gd="$(<"$d/.git")"; gd="${gd#gitdir: }"
      [[ "$gd" != /* ]] && gd="$d/$gd"
      [[ -f "$gd/HEAD" ]] && head="$(<"$gd/HEAD")"
      break
    fi
    [[ "$d" == "/" || -z "$d" ]] && break
    d="${d:h}"
  done
  [[ -z "$head" ]] && return
  if [[ "$head" == ref:* ]]; then REPLY="${head#ref: refs/heads/}"; else REPLY="${head[1,7]}"; fi
}

__ms_load_state() {
  [[ -r "$__ms_pfile" ]] || return
  local line k v c ok
  for line in "${(@f)$(<"$__ms_pfile")}"; do
    k="${line%%=*}"; v="${line#*=}"
    case "$k" in
      theme) [[ "$v" == (none|two-line|minimal|blocks) ]] && __ms_theme="${v:#none}" ;;
      gradient) [[ "$v" == (0|1) ]] && __ms_grad="$v" ;;
      colors)
        local -a cs=("${(@s:,:)v}")
        ok=1
        for c in "${cs[@]}"; do [[ "$c" =~ '^#[0-9a-fA-F]{6}$' ]] || ok=0; done
        (( ok && ${#cs} == 16 )) && __ms_hex=("${cs[@]}")
        ;;
    esac
  done
}

# "#rrggbb" "#rrggbb" t(0..1000) -> REPLY = blended "#rrggbb"
__ms_mix() {
  local a="$1" b="$2" t=$3
  local r=$(( 16#${a[2,3]} + (16#${b[2,3]} - 16#${a[2,3]}) * t / 1000 ))
  local g=$(( 16#${a[4,5]} + (16#${b[4,5]} - 16#${a[4,5]}) * t / 1000 ))
  local bl=$(( 16#${a[6,7]} + (16#${b[6,7]} - 16#${a[6,7]}) * t / 1000 ))
  printf -v REPLY '#%02x%02x%02x' $r $g $bl
}

# Render powerline segments (arrays sb=bg index, sf=fg index, st=text).
# Solid mode uses palette indexes; gradient mode paints each cell with a
# truecolor background blended from this segment's color toward the next.
__ms_render_blocks() {
  local sep=$'' out="" i n=${#st}
  if (( __ms_grad && ${#__ms_hex} == 16 )); then
    local -a from to
    for (( i = 1; i <= n; i++ )); do
      from[i]="${__ms_hex[sb[i]+1]}"
      if (( i < n )); then
        __ms_mix "${from[i]}" "${__ms_hex[sb[i+1]+1]}" 700
      else
        __ms_mix "${from[i]}" "#ffffff" 300
      fi
      to[i]="$REPLY"
    done
    local text ch j len
    for (( i = 1; i <= n; i++ )); do
      (( i > 1 )) && out+="%K{${from[i]}}%F{${to[i-1]}}${sep}"
      out+="%F{${sf[i]}}"
      text=" ${(%)st[i]} "
      len=${#text}
      for (( j = 1; j <= len; j++ )); do
        __ms_mix "${from[i]}" "${to[i]}" $(( len > 1 ? (j - 1) * 1000 / (len - 1) : 0 ))
        ch="${text[j]}"
        # Literal text (e.g. a folder named '$(cmd)') must never be expanded,
        # even under PROMPT_SUBST: escape prompt and substitution characters.
        case "$ch" in
          %) ch="%%" ;;
          '$'|'`'|'\') ch="\\$ch" ;;
        esac
        out+="%K{$REPLY}$ch"
      done
    done
    PROMPT="${out}%k%F{${to[n]}}${sep}%f "
  else
    for (( i = 1; i <= n; i++ )); do
      (( i > 1 )) && out+="%K{${sb[i]}}%F{${sb[i-1]}}${sep}"
      out+="%K{${sb[i]}}%F{${sf[i]}} ${st[i]} "
    done
    PROMPT="${out}%k%F{${sb[n]}}${sep}%f "
  fi
}

__ms_prompt() {
  local ret=$1 took="" branch="" venv=""
  if [[ -n "$__ms_t0" ]]; then
    local secs=$(( EPOCHREALTIME - __ms_t0 ))
    __ms_t0=""
    if (( secs >= 2 )); then
      if (( secs >= 60 )); then took="$(( ${secs%.*} / 60 ))m$(( ${secs%.*} % 60 ))s"; else took="$(printf '%.1fs' $secs)"; fi
    fi
  fi
  __ms_git_branch; branch="$REPLY"
  [[ -n "$VIRTUAL_ENV" ]] && venv="${VIRTUAL_ENV:t}"
  # Branch/venv names come from disk: keep only safe characters so a name
  # like '$(cmd)' can't run under PROMPT_SUBST (which oh-my-zsh enables).
  branch="${branch//[^A-Za-z0-9._\/+-]/?}"
  venv="${venv//[^A-Za-z0-9._+-]/?}"
  local dir='%F{4}%B%(4~|…/%3~|%~)%b%f'
  local seg=""
  [[ -n "$branch" ]] && seg+=" %F{8}on%f %F{5}⎇ ${branch}%f"
  [[ -n "$venv" ]] && seg+=" %F{8}via%f %F{3}◆ ${venv}%f"
  [[ "$ret" != 0 ]] && seg+=" %F{1}✗ $ret%f"
  [[ -n "$took" ]] && seg+=" %F{8}⏱ $took%f"
  case "$__ms_theme" in
    two-line)
      PROMPT=$'\n'"%F{8}╭─%f %F{6}%n%f%F{8}@%m%f ${dir}${seg}"$'\n'"%F{8}╰─%f%(?.%F{2}.%F{1})❯%f "
      ;;
    minimal)
      PROMPT="${dir}${seg} %(?.%F{2}.%F{1})❯%f "
      ;;
    blocks)
      # Powerline / agnoster style: colored segments joined by  arrows
      # (xterm.js draws the arrow glyph itself, no patched font needed).
      local -a sb sf st
      local status_txt="" status_fg=6 jobs="${(%):-%j}"
      [[ "$ret" != 0 ]] && { status_txt+="✘ $ret "; status_fg=1; }
      (( UID == 0 )) && { status_txt+="root "; [[ $status_fg == 6 ]] && status_fg=3; }
      (( jobs > 0 )) && status_txt+="⚙ $jobs "
      [[ -n "$status_txt" ]] && { sb+=(0); sf+=($status_fg); st+=("${status_txt% }"); }
      [[ -n "$SSH_CONNECTION" ]] && { sb+=(0); sf+=(7); st+=('%n@%m'); }
      sb+=(4); sf+=(0); st+=('%(4~|…/%3~|%~)')
      [[ -n "$branch" ]] && { sb+=(2); sf+=(0); st+=("⎇ ${branch}"); }
      [[ -n "$venv" ]] && { sb+=(3); sf+=(0); st+=("◆ ${venv}"); }
      [[ -n "$took" ]] && { sb+=(8); sf+=(15); st+=("⏱ $took"); }
      __ms_render_blocks
      ;;
  esac
  RPROMPT=""
}

if [[ -n "$__ms_theme" ]]; then
  # Colorful ls and a nicer completion menu, like oh-my-zsh defaults.
  export CLICOLOR=1
  export LSCOLORS="${LSCOLORS:-ExGxFxdaCxDaDahbadacec}"
  zstyle ':completion:*' menu select
  zstyle ':completion:*' list-colors 'di=1;34' 'ln=1;36' 'so=35' 'pi=33' 'ex=1;32'
  zstyle ':completion:*' matcher-list 'm:{a-z}={A-Za-z}'
fi

autoload -Uz add-zsh-hook
add-zsh-hook preexec __ms_preexec
# precmd must run last so other hooks (themes) have already built PS1.
precmd_functions=(${precmd_functions:#__ms_precmd} __ms_precmd)
