(ns proserunner.output.format
  "Formatters for grouped, table, and verbose output."
  (:gen-class)
  (:require [clojure.set :refer [rename-keys]]
            [clojure.string :as string]
            [clojure.java.io :as io]
            [proserunner.output.prep :as prep]))

(set! *warn-on-reflection* true)

;;; Grouped output format

(defn group-numbered
  "Groups results by file with issue numbers, preserving order."
  [results]
  (let [indexed-results (map-indexed (fn [idx issue] [(inc idx) issue]) results)
        ;; Widths span every issue so each file's block shares the same columns
        widths (prep/column-widths indexed-results)
        ;; Group by file but preserve order using partition-by
        grouped (partition-by (fn [[_ issue]] (:file issue)) indexed-results)]
    (doseq [file-group grouped]
      (when-let [first-item (first file-group)]
        (println)
        (println (:file (second first-item)))
        (doseq [[num issue] file-group]
          (println (prep/issue-str num issue widths)))))))

;;; Formatters for command output

(defn ignored-list
  "Formats separated ignore lists for display.
   Takes map with :ignore (set of strings) and :ignore-issues (vector of maps)."
  [{:keys [ignore ignore-issues]}]
  (let [has-simple? (seq ignore)
        has-contextual? (seq ignore-issues)]
    (cond
      (and (not has-simple?) (not has-contextual?))
      ["No ignored specimens or issues."]

      :else
      (concat
        (when has-simple?
          (cons "Simple ignores (apply everywhere):"
                (concat
                  (map #(str "  - " %) (sort ignore))
                  [""])))
        (when has-contextual?
          (cons "Contextual ignores (specific locations):"
                (map (fn [{:keys [file line-num line specimen]}]
                       (format "  - %s:%s \"%s\""
                               file
                               (or line-num line "*")
                               specimen))
                     (sort-by (juxt :file #(or (:line-num %) (:line %) 0) :specimen)
                              ignore-issues))))))))

(defn init-project
  "Formats project initialization success message."
  [_]
  ["Created project configuration directory: .proserunner/"
   "  + .proserunner/config.edn - Project configuration"
   "  + .proserunner/checks/    - Directory for project-specific checks"
   ""
   "Edit .proserunner/config.edn to customize:"
   "  :checks        - Specify checks (vector of string references or maps):"
   "                   - \"default\" or \"custom\" to reference global checks"
   "                   - {:directory \"checks\"} for .proserunner/checks/"
   "                   - {:directory \"path\" :files [...]} for custom"
   "  :ignore        - Set of specimens to ignore"
   "  :ignore-mode   - :extend (merge with global) or :replace"
   "  :config-mode   - :merged (use global+project) or :project-only"
   ""
   "Add custom checks by creating .edn files in .proserunner/checks/"])

;;; Table printing utilities

(defn make-key-printer
  "Used for printing in a table: makes for sentence-case column headings."
  [km]
  (fn [m] (rename-keys m km)))

(defn- make-row-formatter
  [ks fmts]
  (fn
    [leader divider trailer row]
    (str leader
         (apply str (interpose divider
                               (for [[col fmt] (map vector (map #(get row %) ks) fmts)]
                                 (format fmt (str col)))))
         trailer)))

(defn- col-widths
  "Determines column widths for spacing tables."
  [ks rows]
  (map
   (fn [k]
     (apply max (count (str k))
            (map #(count (str (get % k))) rows)))
   ks))

(defn print-table
  "Prints a collection of maps as a left-justified table with borders.
  Prints table headings ks, and then a line of output for each row."
  ([ks rows]
   (when (seq rows)
     (let [widths (col-widths ks rows)
           spacers (map #(apply str (repeat % "-")) widths)
           fmts (map #(str "%-" % "s") widths)
           fmt-row (make-row-formatter ks fmts)]
       (println)
       (println (fmt-row "| " " | " " |" (zipmap ks ks)))
       (println (fmt-row "|-" "-+-" "-|" (zipmap ks spacers)))
       (doseq [row rows]
         (println (fmt-row "| " " | " " |" row))))))
  ([rows] (print-table (keys (first rows)) rows)))

(defn print-options
  "Prints a collection of maps as a left-justified table without borders."
  ([ks rows]
   (when (seq rows)
     (let [widths (col-widths ks rows)
           fmts (map #(str "%-" % "s") widths)
           fmt-row (make-row-formatter ks fmts)]
       (println)
       (doseq [row rows]
         (println (fmt-row "  " "   " "  " row))))))
  ([rows] (print-table (keys (first rows)) rows)))

;;; Table output with issue numbers

(defn table-numbered
  "Takes results and prints them as a table with issue numbers."
  [results]
  (->> results
       (map-indexed (fn [idx issue]
                      (assoc issue :issue-num (inc idx))))
       (map (make-key-printer {:issue-num "#"
                               :file "File"
                               :line-num "Line"
                               :col-num "Col"
                               :specimen "Specimen"
                               :name "Name"
                               :message "Message"}))
       (print-table)))

;;; Verbose output format

(def check-kind-guidance
  "Generic guidance for each check type to help users understand how to fix issues."
  {"existence" "Remove or rephrase."
   "case" "Remove or rephrase (case-sensitive)."
   "recommender" "Replace with the preferred alternative."
   "case-recommender" "Replace with the preferred alternative (case-sensitive)."
   "repetition" "Remove the duplicate word."
   "regex" "Follow the guidance in the message."})

(defn- verbose-issue
  "Formats a single issue as markdown for verbose output."
  [issue-num {:keys [file line-num col-num specimen line-text name message kind]}]
  (let [kind-lower (string/lower-case kind)
        guidance (get check-kind-guidance kind-lower "Follow the guidance in the message.")
        ;; Convert to absolute path
        abs-file (-> file
                     (string/replace "~" (System/getProperty "user.home"))
                     io/file
                     .getAbsolutePath)]
    (str "### " issue-num ". " name " (" kind-lower ")\n\n"
         "`" abs-file ":" line-num ":" col-num "`\n\n"
         "**Line:** " line-text "\n\n"
         "**Problem:** `" specimen "`"
         (when (and (or (= kind-lower "recommender")
                       (= kind-lower "case-recommender"))
                   (string/starts-with? message "Prefer:"))
           (str " → `" (string/trim (subs message 7)) "`"))
         " - " message "\n\n"
         "**How to fix:** " guidance "\n")))

(defn verbose
  "Formats results as verbose markdown output."
  [results]
  (let [file-count (count (distinct (map :file results)))
        issue-count (count results)
        by-kind (group-by :kind results)
        by-file (group-by :file results)]
    (println "# Proserunner Analysis Results\n")
    (println (str "**Files analyzed:** " file-count))
    (println (str "**Issues found:** " issue-count "\n"))
    (println "---\n")
    (println "## Issues\n")
    (doseq [[idx issue] (map-indexed vector results)]
      (println (verbose-issue (inc idx) issue))
      (println "---\n"))
    (println "## Summary\n")
    (println (str "- **Total issues:** " issue-count))
    (println "- **By type:**")
    (doseq [[kind issues] (sort-by first by-kind)]
      (println (str "  - " (string/lower-case kind) ": " (count issues))))
    (println "- **By file:**")
    (doseq [[file issues] (sort-by first by-file)]
      (println (str "  - `" file "`: " (count issues) " issue" (when (not= 1 (count issues)) "s"))))))
