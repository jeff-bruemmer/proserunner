(ns proserunner.ignore.audit
  "Audit and cleanup functionality for stale ignores."
  (:gen-class)
  (:require [clojure.java.io :as io]))

(set! *warn-on-reflection* true)

;;; Audit and Cleanup

(defn file-exists?
  "Checks if a file exists."
  [file-path]
  (when file-path
    (.exists (io/file file-path))))

(defn audit
  "Audits ignore entries to find stale ones (files that don't exist).
   Returns a map with :stale and :active entries.

   Takes map with :ignore (strings, always active) and :ignore-issues (maps, checked for file existence).
   Returns map with :ignore (set) and :ignore-issues (vectors) for both :stale and :active."
  [{:keys [ignore ignore-issues]}]
  (let [stale-issues (filterv (fn [entry]
                                (not (file-exists? (:file entry))))
                              ignore-issues)
        active-issues (filterv (fn [entry]
                                 (file-exists? (:file entry)))
                               ignore-issues)]
    {:stale {:ignore #{}
             :ignore-issues stale-issues}
     :active {:ignore ignore
              :ignore-issues active-issues}}))

(defn format-report
  "Formats an audit result into a human-readable report.
   Returns a vector of strings for display."
  [{:keys [stale active]}]
  (let [total-ignores (+ (count (:ignore active)) (count (:ignore stale)))
        total-issues (+ (count (:ignore-issues active)) (count (:ignore-issues stale)))
        total (+ total-ignores total-issues)
        stale-count (count (:ignore-issues stale))]
    (if (zero? stale-count)
      [(format "✓ All %d ignore entries are active (%d simple, %d contextual)."
               total total-ignores total-issues)
       "No stale ignores found."]
      (vec
       (concat
        [(format "Found %d stale contextual ignore(s) out of %d total entries:" stale-count total)
         ""]
        (map (fn [entry]
               (format "  - File: %s:%s - \"%s\""
                      (:file entry)
                      (or (:line-num entry) (:line entry) "*")
                      (:specimen entry)))
             (:ignore-issues stale))
        [""]
        [(format "Stale ignores reference files that no longer exist.")]
        [(format "Run 'proserunner ignore clean' to remove them.")])))))

(defn remove-stale
  "Removes stale ignores from an ignore map.
   Returns map with only active ignores."
  [ignores]
  (:active (audit ignores)))
