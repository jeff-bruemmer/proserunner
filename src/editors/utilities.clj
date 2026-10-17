(ns editors.utilities
  "Shared utilities for creating editor functions that detect and report issues in text."
  (:require [proserunner.console :as console]
            [proserunner.text :as text]
            [clojure.string :as string])
  (:gen-class))

(set! *warn-on-reflection* true)

(defn safe-make-pattern
  "Safely compile regex pattern to search for multiple specimens at once.
   Group 1 of a match is the specimen. Returns nil if pattern is invalid."
  [re-payload case-sensitive?]
  (try
    ;; Skip specimens right after [, #, _, or -. The - comes last so it's
    ;; literal; between # and _ it would exclude every character in that range.
    (let [ignore-chars "(?:^|[^\\[#_-])"
          boundary "\\b("
          leftb (if case-sensitive?
                  ;; Ignore matches in markdown/org links
                  (str ignore-chars boundary)
                  (str "(?i)" ignore-chars boundary))]
      (->> re-payload
           (#(str leftb % ")\\b"))
           (re-pattern)))
    (catch java.util.regex.PatternSyntaxException e
      (console/warn "Invalid regex pattern '" re-payload "': " (.getMessage e))
      nil)))

(defn safe-re-pattern
  "Safely compile a regex pattern, returning nil if invalid."
  [re]
  (try
    (re-pattern re)
    (catch java.util.regex.PatternSyntaxException e
      (console/warn "Invalid regex pattern '" re "': " (.getMessage e))
      nil)))

(defn specimens-pattern
  "Compiles one pattern that finds any of `specimens`."
  [specimens case-sensitive?]
  (safe-make-pattern (string/join "|" specimens) case-sensitive?))

(defn find-matches
  "Returns [text col] for each match of `pattern` in `text`: the text and
  start of capture `group`. Returns [] when pattern is nil (it didn't compile)."
  [pattern text group]
  (if (nil? pattern)
    []
    (let [^java.util.regex.Matcher m (re-matcher pattern text)
          g (int group)]
      (loop [found []]
        (if (.find m)
          (recur (conj found [(.group m g) (.start m g)]))
          found)))))

(defn seek
  "Returns [specimen col] for each specimen that `pattern`, made by
  safe-make-pattern, finds in `text`."
  [text pattern]
  (find-matches pattern text 1))

(defn create-issue-record
  "Builds an Issue record with normalized file path for consistent reporting."
  [file name kind specimen col message]
  (text/->Issue (text/home-path file) name kind specimen col message))

(defn add-issue-to-line
  "Marks line as having issues and appends the issue to its collection."
  [line issue]
  (-> line
      (assoc :issue? true)
      (update :issues conj issue)))

(defn add-issue
  "Adds an issue for `specimen`, found at column `col`, to a text/Line's issues."
  [{:keys [line specimen col name kind message]}]
  (add-issue-to-line line (create-issue-record (:file line) name kind specimen col message)))

(defn add-matches
  "Adds an issue to `line` for each [specimen col] in `matches`."
  [line matches {:keys [name kind]} message]
  (reduce (fn [l [specimen col]]
            (add-issue {:line l
                        :specimen specimen
                        :col col
                        :name name
                        :kind kind
                        :message message}))
          line
          matches))

(defn create-issue-collector
  "Samples lines for each specimen in check, and adds any issues to the line.
  Uses the check's compiled :pattern (see compile-patterns) when it has one."
  [case-sensitive?]
  (fn
    [line
     {:keys [specimens message pattern] :as check}]
    (if (seq specimens)
      (add-matches line
                   (seek (:text line) (or pattern (specimens-pattern specimens case-sensitive?)))
                   check
                   message)
      line)))

(defn create-recommender
  "Returns a function that accepts a boolean to determine whether the
  recommender is case sensitive."
  [case-sensitive?]
  (fn [line {:keys [recommendations] :as check}]
    (reduce (fn [line {:keys [prefer avoid pattern]}]
              (add-matches line
                           (seek (:text line) (or pattern (safe-make-pattern avoid case-sensitive?)))
                           check
                           (str "Prefer: " prefer)))
            line
            recommendations)))

(def ^:private editor-specs
  "Specifications for creating standard editors.
   Each spec defines the constructor function and case-sensitivity parameter."
  {"existence"        {:factory create-issue-collector :case-sensitive? false}
   "case"             {:factory create-issue-collector :case-sensitive? true}
   "recommender"      {:factory create-recommender :case-sensitive? false}
   "case-recommender" {:factory create-recommender :case-sensitive? true}})

(defn create-editor
  "Factory function to create an editor by type.
   Returns the editor function for the given type, or nil if unknown.

   Example:
     (create-editor \"existence\") => issue collector function (case-insensitive)
     (create-editor \"case\")      => issue collector function (case-sensitive)"
  [editor-type]
  (when-let [{:keys [factory case-sensitive?]} (get editor-specs editor-type)]
    (factory case-sensitive?)))

(defn standard-editor-types
  "Returns a set of all standard editor types available from the factory."
  []
  (set (keys editor-specs)))

(defn compile-patterns
  "Returns `check` with its regexes compiled, as :pattern on the check (for
  specimens), on each recommendation, and on each expression. Compile once
  per run, so editors don't compile them for every line."
  [{:keys [kind specimens recommendations expressions] :as check}]
  (let [case-sensitive? (boolean (get-in editor-specs [kind :case-sensitive?]))]
    (cond-> check
      (seq specimens)
      (assoc :pattern (specimens-pattern specimens case-sensitive?))

      (seq recommendations)
      (assoc :recommendations
             (mapv #(assoc % :pattern (safe-make-pattern (:avoid %) case-sensitive?))
                   recommendations))

      (seq expressions)
      (assoc :expressions
             (mapv #(assoc % :pattern (safe-re-pattern (:re %))) expressions)))))
