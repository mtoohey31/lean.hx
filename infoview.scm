(require-builtin steel/ffi)
(require-builtin steel/time)

(require (prefix-in helix.commands. "helix/commands.scm"))
(require (prefix-in helix.configuration. "helix/configuration.scm"))
(require (prefix-in helix.editor. "helix/editor.scm"))
(require (prefix-in helix.ext. "helix/ext.scm"))
(require (prefix-in helix.misc. "helix/misc.scm"))

(#%require-dylib "liblean_hx"
  (prefix-in lean.hx.
    (only-in server server-listen! unbounded-send oneshot-send)))

(define (server-thread)
  (let*
    [
      (subscription-mutex (mutex))
      (subscription-counts (hash))
      (rpc-mutex (mutex))
      (rpc-sessions (hashset))
      (sender-mutex (mutex))
      (sender void)
      (server-and-sender (lean.hx.server
        (helix.ext.hx.block-on-task
          (lambda ()
            (helix.misc.cursor-lsp-location "utf-16")))

        (helix.ext.hx.block-on-task
          (lambda ()
            (helix.misc.get-lsp-initialize-result "lean")))

        ; send_client_request
        (function->ffi-function
          (lambda (uri method params tx)
            (helix.ext.hx.block-on-task
              (lambda ()
                (helix.misc.send-lsp-command "lean" method params
                  (lambda (result) (lean.hx.oneshot-send tx result)))))))

        ; send_client_notification
        (function->ffi-function
          (lambda (uri method params)
            (helix.ext.hx.block-on-task
              (lambda ()
                (helix.misc.send-lsp-notification "lean" method params)))))

        ; subscribe_server_notifications
        (function->ffi-function
          (lambda (method)
            (let* [(guard (lock-acquire! subscription-mutex))
                   (found (hash-contains? subscription-counts method))
                   (prev (if found (hash-ref subscription-counts method) 0))]
              (set! subscription-counts
                (hash-insert subscription-counts method (+ prev 1)))
              (lock-release! guard)
              (if (not found)
                (helix.ext.hx.block-on-task
                  (lambda ()
                    (helix.configuration.register-lsp-notification-handler
                      "lean"
                      method
                      (lambda (params)
                        (let [(guard (lock-acquire! subscription-mutex))
                              (ok (and
                                    (hash-contains? subscription-counts method)
                                    (> (hash-ref subscription-counts method) 0)))]
                          (lock-release! guard)
                          (if ok
                            (let [(guard (lock-acquire! sender-mutex))]
                              (lean.hx.unbounded-send sender
                                (hash
                                  'type "got_server_notification"
                                  'method method
                                  'params params))
                              (lock-release! guard))))))))))))

        ; unsubscribe_server_notifications
        (function->ffi-function
          (lambda (method)
            (let [(guard (lock-acquire! subscription-mutex))]
              (if (hash-contains? subscription-counts method)
                (let [(prev (hash-ref subscription-counts method))]
                  (if (>= prev 0)
                    (set! subscription-counts
                      (hash-insert subscription-counts method (- prev 1)))
                    (error! "server unsubscribed more times than it subscribed")))
                (error! "server unsubscribed without subscribing"))
              (lock-release! guard))))

        ; create_rpc_session
        (function->ffi-function
          (lambda (uri tx)
            (helix.ext.hx.block-on-task
              (lambda ()
                (helix.misc.send-lsp-command
                  "lean"
                  "$/lean/rpc/connect"
                  (hash 'uri uri)
                  (lambda (result)
                    (let* [(session-id (hash-ref result 'sessionId))
                           (open
                             (lambda ()
                               (let [(guard (lock-acquire! rpc-mutex))
                                     (res (hashset-contains? rpc-sessions session-id))]
                                 (lock-release! guard)
                                 res)))
                           (keep-alive-thread
                             (lambda ()
                               (while (open)
                                 (helix.ext.hx.block-on-task
                                   (lambda ()
                                     (helix.misc.send-lsp-notification "lean"
                                       "$/lean/rpc/keepAlive"
                                       (hash 'uri uri 'sessionId session-id))))
                                 (time/sleep-ms 1000))))]
                      (let [(guard (lock-acquire! rpc-mutex))]
                        (set! rpc-sessions (hashset-insert rpc-sessions session-id))
                        (lock-release! guard))
                      (spawn-native-thread keep-alive-thread)
                      (lean.hx.oneshot-send tx session-id))))))))

        ; close_rpc_session
        (function->ffi-function
          (lambda (session-id)
            (let [(guard (lock-acquire! rpc-mutex))]
              (set! rpc-sessions
                (hashset-difference rpc-sessions (hashset session-id)))
              (lock-release! guard))))))
      ]

      (set! sender (second server-and-sender))

      (helix.ext.hx.block-on-task
        (lambda ()
          (register-hook! "selection-did-change"
            (lambda (view)
              (let [(language
                      (helix.ext.hx.block-on-task
                        (lambda ()
                          (helix.editor.editor-document->language
                            (helix.editor.editor->doc-id view)))))]
                (if (equal? language "lean")
                  (let [(loc
                          (helix.ext.hx.block-on-task
                            (lambda ()
                              (helix.misc.cursor-lsp-location "utf-16"))))
                        (guard (lock-acquire! sender-mutex))]
                    (lean.hx.unbounded-send sender
                      (hash 'type "changed_cursor_location" 'loc loc))
                    (lock-release! guard))))))))

      (lean.hx.server-listen! (first server-and-sender))))

(provide lean-infoview)
(define (lean-infoview)
  (spawn-native-thread server-thread)
  void)
