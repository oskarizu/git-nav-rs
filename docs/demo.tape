# VHS tape script for the git-nav README demo.
#
# Regenerate the gif with:
#   vhs docs/demo.tape        # writes docs/demo.gif
#
# Install VHS (once):
#   brew install vhs          # macOS
#   see https://github.com/charmbracelet/vhs for other platforms
#
# The setup block (between `Hide` and `Show`) fabricates a handful of
# fake repos under /tmp/gn-demo and points git-nav at them via an
# isolated XDG_CONFIG_HOME, so the recording is deterministic and
# doesn't leak any of your real project names.

Output docs/demo.gif

Set Shell "zsh"
Set FontSize 14
Set Width 1000
Set Height 380
Set Theme "Catppuccin Mocha"

Hide
Set TypingSpeed 0ms
Type `rm -rf /tmp/gn-demo /tmp/gn-cfg && mkdir -p /tmp/gn-demo /tmp/gn-cfg/git-nav`
Enter
Type `for r in api-gateway billing-svc billing-worker docs-site git-nav web-portal; do (cd /tmp/gn-demo && mkdir "$r" && cd "$r" && git init -q -b main && git config --local user.email d@e.com && git config --local user.name d && echo x > f && git add f && git commit -q -m init); done`
Enter
Type `printf 'root = "/tmp/gn-demo"\ndepth = 2\n' > /tmp/gn-cfg/git-nav/config.toml`
Enter
Type `export XDG_CONFIG_HOME=/tmp/gn-cfg && PROMPT='%~ %# '`
Enter
Type `eval "$(git-nav --init zsh)"`
Enter
Type "clear"
Enter

Set TypingSpeed 60ms
Show

Sleep 500ms
Type "gnav"
Enter
Sleep 2000ms
Type "bil"
Sleep 1000ms
Down
Sleep 800ms
Enter
Sleep 2000ms
