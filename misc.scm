(provide foldl)
(define (foldl f init lst)
  (if (null? lst) init (foldl f (f init (car lst)) (cdr lst))))

(provide filter)
(define (filter f lst)
  (if
    (null? lst) (list)
    (let ([cdr1 (filter f (cdr lst))])
      (if (f (car lst)) (cons (car lst) cdr1) cdr1))))

(provide reduce)
(define (reduce f lst)
  (if
    (null? lst) #f
    (if (null? (cdr lst))
      (car lst)
      (f (car lst) (reduce f (cdr lst))))))

(provide hash-map)
(define (hash-map f map)
  (foldl
    (lambda (acc key)
      (let ([p (f key (hash-get map key))]) (hash-insert acc (car p) (car (cdr p)))))
    (hash)
    (hash-keys->list map)))

(provide string-shorter)
(define (string-shorter str1 str2) (if (> (string-length str1) (string-length str2)) str2 str1))

(provide split-once1)
(define (split-once1 str sep) (let ([pair (split-once str sep)]) (if (pair? pair) pair (list str))))

(provide read-file-to-string)
(define (read-file-to-string path)
  (let* ([port (open-input-file path)]
         [s (read-port-to-string port)])
    (close-input-port port)
    s))
