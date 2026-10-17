(ns editors.repetition
  "Detects consecutive repeated words in text."
  (:gen-class)
  (:require [editors.utilities :as util]))

(def ^:private repeated-word
  "A word, whitespace, and the same word again, e.g. \"is is\"."
  #"\b(\w+)\s+\1\b")

(defn proofread
  "If proofread finds consecutive repeated words in the `line`,
  proofread creates an issue and adds it to the line's issues vector."
  [line check]
  (util/add-matches line
                    (util/find-matches repeated-word (:text line) 0)
                    check
                    (:message check)))
