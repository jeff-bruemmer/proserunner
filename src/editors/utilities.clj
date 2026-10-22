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
  "Safely compile a regex pattern, with optional java.util.regex.Pattern
  `flags`, returning nil if invalid."
  ([re] (safe-re-pattern re 0))
  ([^String re flags]
   (try
     (java.util.regex.Pattern/compile re (int flags))
     (catch java.util.regex.PatternSyntaxException e
       (console/warn "Invalid regex pattern '" re "': " (.getMessage e))
       nil))))

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

(defn- avoid-pattern
  "Compiles a recommendation's `avoid` by itself, to tell which
  recommendation a match of the check's combined pattern came from."
  [avoid case-sensitive?]
  (safe-re-pattern avoid (if case-sensitive? 0 java.util.regex.Pattern/CASE_INSENSITIVE)))

(defn compile-recommendations
  "Returns {:pattern p :recommendations rs}: p, made by safe-make-pattern,
  finds any recommendation's avoid, and each of rs has its avoid-pattern as
  :pattern. Drops recommendations whose avoid doesn't compile, so one bad
  avoid doesn't disable the rest of the check."
  [recommendations case-sensitive?]
  (let [rs (->> recommendations
                (map #(assoc % :pattern (avoid-pattern (:avoid %) case-sensitive?)))
                (filterv :pattern))]
    {:pattern (when (seq rs)
                (safe-make-pattern (string/join "|" (map :avoid rs)) case-sensitive?))
     :recommendations rs}))

(defn- matches-span?
  "True when `pattern` matches exactly `specimen`, found at `col` in `text`.
  Lookarounds still see the rest of the line."
  [pattern ^String text ^String specimen col]
  (-> (re-matcher pattern text)
      (.region (int col) (+ (int col) (count specimen)))
      (.useTransparentBounds true)
      (.useAnchoringBounds false)
      .matches))

(defn create-recommender
  "Returns a recommender editor, case sensitive or not. One pass of the
  check's combined pattern (see compile-recommendations) finds every avoid;
  each match is reported for each recommendation whose avoid it is."
  [case-sensitive?]
  (fn [line check]
    (let [{:keys [pattern recommendations]}
          (if (:pattern check)
            check
            (compile-recommendations (:recommendations check) case-sensitive?))
          text (:text line)]
      (reduce (fn [line [specimen col :as match]]
                (reduce #(add-matches %1 [match] check (str "Prefer: " (:prefer %2)))
                        line
                        (filter #(matches-span? (:pattern %) text specimen col) recommendations)))
              line
              (seek text pattern)))))

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
  "Returns `check` with its regexes compiled: :pattern on the check (for
  specimens, or all recommendations; see compile-recommendations), and on
  each recommendation and expression. Compile once per run, so editors
  don't compile them for every line."
  [{:keys [kind specimens recommendations expressions] :as check}]
  (let [case-sensitive? (boolean (get-in editor-specs [kind :case-sensitive?]))]
    (cond-> check
      (seq specimens)
      (assoc :pattern (specimens-pattern specimens case-sensitive?))

      (seq recommendations)
      (merge (compile-recommendations recommendations case-sensitive?))

      (seq expressions)
      (assoc :expressions
             (mapv #(assoc % :pattern (safe-re-pattern (:re %))) expressions)))))
