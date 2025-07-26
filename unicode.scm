(require (prefix-in helix.components. "helix/components.scm"))
(require (prefix-in helix.editor. "helix/editor.scm"))
(require (prefix-in helix.misc. "helix/misc.scm"))
(require (prefix-in helix.static. "helix/static.scm"))
(require "misc.scm")

(define abbrs
  (let ([abbrs-path
          (string-append
            (parent-name (helix.static.get-init-scm-path))
            "/cogs/lean.hx/vendor/vscode-lean4/lean4-unicode-input/src/abbreviations.json")])
    (hash-map
      (lambda (abbr expansion) (list (symbol->string abbr) (split-once1 expansion "$CURSOR")))
      (string->jsexpr (read-file-to-string abbrs-path)))))

(define (abbrs-matching input)
  (filter (lambda (abbr) (starts-with? abbr input)) (hash-keys->list abbrs)))

(define state "")
(define inlay-ids #f)

(define (expand abbr)
  (let ([expansion (hash-get abbrs abbr)])
    (if (equal? (length expansion) 1)
      (helix.static.insert_string (car expansion))
      (let ([rhs (car (cdr expansion))])
        (helix.static.insert_string (string-append (car expansion) rhs))
        (repeat helix.static.move_char_left (length (string->list rhs)))))))

(define (add-inlay-hint)
  (set! inlay-ids
    (helix.misc.add-inlay-hint
      (helix.misc.cursor-position)
      (string-append "\\" state))))

(define (clear-inlay-hint)
  (if inlay-ids
    (let ()
      (helix.misc.remove-inlay-hint-by-id (car inlay-ids) (car (cdr inlay-ids)))
      (set! inlay-ids #f))
    '()))

(define (update-inlay-hint)
  (clear-inlay-hint)
  (add-inlay-hint))

(define (expand-state)
  (let ([matching-abbrs (abbrs-matching state)])
    (if (equal? (length matching-abbrs) 0)
      (helix.misc.set-error!
        (string-append "no unicode abbreviation begins with \"" state "\""))
      (if (equal? (string-length state) 0)
        (helix.misc.set-error! "empty unicode abbreviation input")
        (expand (reduce string-shorter matching-abbrs)))))
  (set! state "")
  (clear-inlay-hint)
  helix.components.event-result/ignore-and-close)

(provide lean-unicode)
(define (lean-unicode)
  (add-inlay-hint)
  (helix.misc.push-component!
    (helix.components.new-component!
      "lean-unicode"
      #f
      (lambda (_ _ _) '())
      (hash
        "handle_event"
        (lambda (_ event)
          (let ([char (helix.components.key-event-char event)])
            (cond
              [(helix.components.key-event-escape? event)
               (set! state "")
               (clear-inlay-hint)
               helix.components.event-result/close]
              [(helix.components.key-event-backspace? event)
               (if (equal? state "")
                 (let ()
                   (clear-inlay-hint)
                   helix.components.event-result/close)
                 (let ()
                   (set! state (substring state 0 (- (string-length state) 1)))
                   (update-inlay-hint)
                   helix.components.event-result/consume))]
              [char
               (let* ([state1 (string-append state (make-string 1 char))]
                      [matching-abbrs (abbrs-matching state1)])
                 (if (equal? (length matching-abbrs) 0)
                   (expand-state)
                   (if (and (equal? (length matching-abbrs) 1) (hash-contains? abbrs state1))
                     (let ()
                       (expand state1)
                       (set! state "")
                       (clear-inlay-hint)
                       helix.components.event-result/close)
                     (let ()
                       (set! state state1)
                       (update-inlay-hint)
                       helix.components.event-result/consume))))]
              [else helix.components.event-result/consume])))))))
