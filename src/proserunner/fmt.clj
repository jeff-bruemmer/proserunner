(ns proserunner.fmt
  "Utilities for formatting text."
  (:gen-class)
  (:require [clojure.string :as string]))

;;;; Utitlies

(defn ^:private capitalize-first-char
  "Like string/capitalize, only it ignores the rest of the string
  to retain case-sensitive recommendations."
  [s]
  (if (< (count s) 2)
    (string/upper-case s)
    (str (string/upper-case (subs s 0 1))
         (subs s 1))))

(defn ^:private add-period
  "Ensures a `phrase` ends with a period."
  [phrase]
  (if (string/ends-with? phrase ".")
    phrase
    (str phrase ".")))

(defn sentence-dress
  "Make text look like a proper sentence."
  [text]
  ((comp capitalize-first-char
         add-period) text))

(defn summary
  "Function supplied to cli/parse-opts to format map of command line options.
   Map produced sent to ship/print-options."
  [summary]
  (->> summary
       ;; combine short and long options, aligning long-only options
       (map #(assoc % :option (if (:short-opt %)
                                (str (:short-opt %) ", " (:long-opt %))
                                (str "    " (:long-opt %)))))
       (map #(dissoc % :short-opt :id :validate-fn :validate-msg))))

