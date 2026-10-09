(ns editors.utilities
  "Shared utilities for creating editor functions that detect and report issues in text."
  (:require [proserunner.console :as console]
            [proserunner.text :as text]
            [clojure.string :as string])
  (:gen-class))

;; Pattern compilation without caching for optimal parallel performance.
;; Cache overhead (atom contention) exceeds benefits under parallel workloads.

(defn safe-make-pattern
  "Safely compile regex pattern to search for multiple specimens at once.
   Returns nil if pattern is invalid."
  [re-payload case-sensitive?]
  (try
    (let [ignore-chars "(?:^|[^\\[\\#-_])"
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

(defn seek
  "Given a text, regex, and case sensitivity, returns a vector of regex matches.
   Returns empty vector if pattern is invalid."
  [text re-payload case-sensitive?]
  (let [p (safe-make-pattern re-payload case-sensitive?)]
    (if (nil? p)
      []
      (let [matches (re-seq p text)]
        (if matches
          (into [] (map second matches))
          [])))))

(defn find-column-position
  "Locates specimen in text, trying case-sensitive match first, then case-insensitive.
   Guards against false positives from legitimate repetition."
  [text specimen]
  (or (when (string/includes? text specimen)
        (string/index-of text specimen))
      (when-let [idx (string/index-of (string/lower-case text)
                                     (string/lower-case specimen))]
        idx)))

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
  "Adds an issue to a text/Line's issues."
  [{:keys [line specimen name kind message]}]
  (let [{:keys [file text]} line
        col (find-column-position text specimen)]
    (if (nil? col)
      line
      (add-issue-to-line line (create-issue-record file name kind specimen col message)))))

(defn create-issue-collector
  "Samples lines for each specimen in check, and adds any issues to the line."
  [case-sensitive?]
  (fn
    [line
     {:keys [file kind specimens message name]}]
    (if (seq specimens)
      (let [re-core (string/join "|" specimens)
            matches (seek (:text line) re-core case-sensitive?)]
        (if (seq matches)
          (reduce
           (fn [l match] (add-issue {:file file
                                    :line l
                                    :specimen match
                                    :name name
                                    :kind kind
                                    :message message}))
           line
           matches)
          line))
      line)))

(defn create-recommender
  "Returns a function that accepts a boolean to determine whether the
  recommender is case sensitive."
  [case-sensitive?]
  (fn [line check]
    (let [{:keys [recommendations name kind]} check]
      (if (seq recommendations)
        (reduce (fn [line {:keys [prefer avoid]}]
                  (let [{:keys [file text]} line
                        matches (seek text avoid case-sensitive?)]
                    (if (seq matches)
                      (reduce (fn [l match] (add-issue {:file file
                                                       :line l
                                                       :specimen match
                                                       :name name
                                                       :kind kind
                                                       :message (str "Prefer: " prefer)}))
                              line
                              matches)
                      line)))
                line
                recommendations)
        line))))

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
