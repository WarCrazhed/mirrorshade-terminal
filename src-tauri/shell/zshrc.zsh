__ms_source .zshrc
[[ -o interactive ]] && . "$MIRRORSHADE_INTEGRATION_DIR/mirrorshade.zsh"
# Non-login shells never read .zlogin: restore the user's ZDOTDIR now.
if [[ ! -o login ]]; then
  ZDOTDIR="$MIRRORSHADE_USER_ZDOTDIR"
  unset MIRRORSHADE_USER_ZDOTDIR MIRRORSHADE_INTEGRATION_DIR
  unfunction __ms_source 2>/dev/null
fi
