(require "mattwparas-helix-package/cogs/keymaps.scm")

(define keybindings (deep-copy-global-keybindings))
(define lean-keybindings (hash "insert" (hash "\\" ':lean-unicode)))
(merge-keybindings keybindings lean-keybindings)

(set-global-buffer-or-extension-keymap (hash "lean" keybindings))
