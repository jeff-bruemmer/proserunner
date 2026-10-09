(ns editors.re
  "Applies regex pattern-based checks to text."
  (:require
   [editors.utilities :as util]
   [proserunner.console :as console])
  (:gen-class))

;; Pattern compilation without caching for optimal parallel performance.
;; Direct re-pattern compilation is faster than cache overhead under parallel load.

(defn safe-re-pattern
  "Safely compile a regex pattern, returning nil if invalid."
  [re]
  (try
    (re-pattern re)
    (catch java.util.regex.PatternSyntaxException e
      (console/warn "Invalid regex pattern '" re "': " (.getMessage e))
      nil)))

(defn handle-specimen
  "Re-seq may return either a string or a vector of string matches."
  [specimen]
  (if (= (type specimen) clojure.lang.PersistentVector)
    (first specimen)
    specimen))

(defn find-expression-matches
  "Searches text for regex pattern. Safe against invalid patterns from user-defined checks."
  [text pattern]
  (when pattern
    (re-seq pattern text)))

(defn process-matches
  "Converts regex matches into issue records, handling both string and vector captures."
  [line matches file name kind message]
  (reduce (fn [l m]
            (util/add-issue {:file file
                             :line l
                             :specimen (handle-specimen m)
                             :name name
                             :kind kind
                             :message message}))
          line
          matches))

(defn apply-expression
  "Tests a single regex expression against a line, accumulating any issues found."
  [line expression check]
  (let [{:keys [file text]} line
        {:keys [name kind]} check
        {:keys [re message]} expression
        pattern (safe-re-pattern re)]
    (if-let [matches (find-expression-matches text pattern)]
      (process-matches line matches file name kind message)
      line)))

(defn proofread
  [line check]
  (let [{:keys [expressions]} check]
    (if (seq expressions)
      (reduce #(apply-expression %1 %2 check) line expressions)
      line)))
