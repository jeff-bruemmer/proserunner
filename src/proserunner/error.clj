(ns proserunner.error
  "Utilities for error messages"
  (:gen-class)
  (:require [proserunner.console :as console]))

(set! *warn-on-reflection* true)

(def help-hint "Run 'proserunner --help' for usage.")

;;;; Did-you-mean suggestions for unknown options

(defn- levenshtein
  "Edit distance between strings a and b."
  [a b]
  (let [bv (vec b)
        n (count bv)]
    (peek
     (reduce (fn [prev [i ca]]
               (reduce (fn [row j]
                         (conj row (min (inc (peek row))
                                        (inc (nth prev (inc j)))
                                        (+ (nth prev j)
                                           (if (= ca (nth bv j)) 0 1)))))
                       [(inc i)]
                       (range n)))
             (vec (range (inc n)))
             (map-indexed vector a)))))

(defn suggest-option
  "Returns the long option closest to `unknown`, or nil when none is close
  enough to be a likely typo."
  [unknown long-opts]
  (let [candidate (->> long-opts
                       (map (fn [opt] [opt (levenshtein unknown opt)]))
                       (sort-by second)
                       first)]
    (when (and candidate (<= (second candidate) 2))
      (first candidate))))

(defn- unknown-option
  "Extracts the flag from a tools.cli 'Unknown option' error, or nil."
  [msg]
  (second (re-find #"^Unknown option: \"([^\"]+)\"" msg)))

(defn describe
  "Rewrites tools.cli error strings for people, adding a suggestion
  for unknown options that look like typos."
  [errors long-opts]
  (map (fn [e]
         (if-let [opt (unknown-option e)]
           (str "unknown option \"" opt "\"."
                (when-let [s (suggest-option opt long-opts)]
                  (str " Did you mean " s "?")))
           e))
       errors))

;;;; Printing

(defn message
  "Prints `errors` to stderr, followed by a pointer to --help."
  [errors]
  (doseq [e errors]
    (console/error e))
  (console/warn help-hint))
