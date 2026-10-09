(ns proserunner.output
  "Main output orchestration and formatting dispatch."
  (:gen-class)
  (:require [clojure.pprint :as pp]
            [clojure.string :as string]
            [cheshire.core :as json]
            [proserunner.checks :as checks]
            [proserunner.ignore.core :as ignore-core]
            [proserunner.output.format :as format]
            [proserunner.output.prep :as prep]))

(set! *warn-on-reflection* true)

;;; Main output orchestration

(defn- load-ignore-set
  "Loads ignore map from project config or file.
   Returns map with :ignore (set of strings) and :ignore-issues (set of maps)."
  [project-ignore project-ignore-issues check-dir config]
  (if (some? project-ignore)
    {:ignore project-ignore
     :ignore-issues (or project-ignore-issues #{})}
    (checks/load-ignore-set! check-dir (:ignore config))))

(defn- filter-ignored
  "Removes ignored issues from results using contextual ignore matching.
   Handles both simple specimen ignores and contextual ignores (file+line+specimen+check)."
  [results ignore-map]
  (if (and (empty? (:ignore ignore-map)) (empty? (:ignore-issues ignore-map)))
    results
    (ignore-core/filter-issues results ignore-map)))

(defn- process-results
  "Preps, filters, and sorts results."
  [results ignore-map]
  (->> results
       (mapcat prep/prep)
       (#(filter-ignored % ignore-map))
       (sort-by (juxt :file :line-num :col-num))))

(defn- format-output
  "Formats results according to output format with issue numbers."
  [results output]
  (case (string/lower-case output)
    "edn" (pp/pprint results)
    "json" (do (json/generate-stream results *out*)
               (newline))
    "group" (format/group-numbered results)
    "plain" (format/plain-numbered results)
    "verbose" (format/verbose results)
    (format/table-numbered results)))

(defn out
  "Takes results, preps them, removes specimens to ignore, and
  prints them in the supplied output format.
  Returns the number of issues printed."
  [payload]
  (let [{:keys [results output check-dir config project-ignore project-ignore-issues]} payload]
    (if (empty? results)
      0
      (let [ignore-map (load-ignore-set project-ignore project-ignore-issues check-dir config)
            final-results (process-results (:results results) ignore-map)]
        (format-output final-results output)
        (count final-results)))))

(defn summary
  "One line describing a run, e.g. \"Found 5 issues.\""
  [issue-count]
  (cond
    (zero? issue-count) "No issues found."
    (= 1 issue-count) "Found 1 issue."
    :else (str "Found " issue-count " issues.")))

;; Re-export time-elapsed for backward compatibility with core.clj
(def time-elapsed prep/time-elapsed)
