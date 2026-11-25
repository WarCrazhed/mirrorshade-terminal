# Mirrorshade Terminal shell integration — loaded via ZDOTDIR. Sources the user's real
# startup files from their own ZDOTDIR, then installs the Mirrorshade hooks.
MIRRORSHADE_USER_ZDOTDIR="${MIRRORSHADE_USER_ZDOTDIR:-$HOME}"
__ms_source() {
  local f="$MIRRORSHADE_USER_ZDOTDIR/$1"
  if [[ -r "$f" ]]; then
    ZDOTDIR="$MIRRORSHADE_USER_ZDOTDIR"
    . "$f"
    # The user's file may itself change ZDOTDIR; honor that for later files.
    MIRRORSHADE_USER_ZDOTDIR="$ZDOTDIR"
    ZDOTDIR="$MIRRORSHADE_INTEGRATION_DIR"
  fi
}
__ms_source .zshenv
