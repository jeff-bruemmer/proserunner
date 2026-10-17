(ns editors.re
  "Applies regex pattern-based checks to text."
  (:require
   [editors.utilities :as util])
  (:gen-class))

(defn apply-expression
  "Tests a single regex expression against a line, accumulating any issues found.
  Each issue is the whole match. Uses the expression's compiled :pattern
  (see util/compile-patterns) when it has one."
  [line {:keys [re message pattern]} check]
  (util/add-matches line
                    (util/find-matches (or pattern (util/safe-re-pattern re)) (:text line) 0)
                    check
                    message))

(defn proofread
  [line check]
  (reduce #(apply-expression %1 %2 check) line (:expressions check)))
