(require (prefix-in helix.misc. "helix/misc.scm"))
(require (prefix-in helix.static. "helix/static.scm"))
(require "misc.scm")
(require "mattwparas-helix-package/cogs/helix-ext.scm")

(define abbrs
  (let ([abbrs-path
          (string-append
            (parent-name (helix.static.get-init-scm-path))
            "/cogs/lean.hx/vendor/vscode-lean4/lean4-unicode-input/src/abbreviations.json")])
    (hash-map
      (lambda (abbr expansion) (list (symbol->string abbr) (split-once1 expansion "$CURSOR")))
      (string->jsexpr (read-file-to-string abbrs-path)))))

(define (expand input)
  (let ([matching-abbrs (filter (lambda (abbr) (starts-with? abbr input)) (hash-keys->list abbrs))])
    (if (equal? (length matching-abbrs) 0)
      (helix.misc.set-error! (string-append "no unicode abbreviation begins with \"" input "\""))
      (if (equal? (string-length input) 0)
        (helix.misc.set-error! "empty unicode abbreviation input")
        (let ([expansion (hash-get abbrs (reduce string-shorter matching-abbrs))])
          ; TODO: Surrounding expansions
          (helix.static.insert_string (car expansion)))))))

; TODO: Eager replacement
(provide lean-unicode)
(define (lean-unicode) (helix.misc.push-component! (prompt "unicode: " expand)))
