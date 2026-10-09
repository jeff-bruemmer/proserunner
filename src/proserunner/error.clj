(ns proserunner.error
  "Utilities for error messages"
  (:gen-class)
  (:require [proserunner.console :as console]
            [proserunner.fmt :as fmt]
            [proserunner.result :as result]
            [clojure.string :as string]))

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

;;;; Printing and exiting

(defn message
  "Prints `errors` to stderr, followed by a pointer to --help."
  [errors]
  (doseq [e errors]
    (console/error e))
  (console/warn help-hint))

;; Result-returning alternatives (preferred for testability and composability)

(defn message-result
  "Returns a Result Failure containing error messages.

  Alternative to `message` that returns a Result instead of printing.
  Useful for testing and composable error handling.

  Example:
    (message-result [\"Error 1\" \"Error 2\"])
    ;; => Failure with formatted error message"
  [errors]
  (result/err (str \newline (string/join \newline errors) \newline)
              {:errors errors}))

(defn exit-result
  "Returns a Result Failure instead of exiting.

  Alternative to `exit` that returns a Result for composable error handling.

  Example:
    (exit-result)
    (exit-result \"Custom error message\")
    (exit-result \"Error\" {:code 404})"
  ([]
   (result/err help-hint {}))
  ([msg]
   (result/err (str (fmt/sentence-dress msg) \newline help-hint)
               {:message msg}))
  ([msg context]
   (result/err (str (fmt/sentence-dress msg) \newline help-hint)
               (assoc context :message msg))))

(defn inferior-input-result
  "Returns a Result Failure for invalid input.

  Alternative to `inferior-input` that returns a Result instead of exiting.
  Enables testing and error recovery without process termination.

  Example:
    (inferior-input-result [\"Invalid option: --foo\"])
    ;; => Failure with formatted error"
  [errors]
  (result/err (str "Invalid input:" \newline (string/join \newline errors))
              {:errors errors}))
