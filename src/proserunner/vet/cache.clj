(ns proserunner.vet.cache
  "Caching validation and incremental computation for prose checking.

   Provides three-level caching strategy to minimize redundant work:

   1. Full cache hit - No changes to file, checks, or config
      → Reuse entire cached result (valid-result?)

   2. Partial cache hit - Only file content changed, checks/config unchanged
      → Incremental recompute: process only changed lines (compute-changed)
      → Reuse cached results for unchanged lines

   3. Cache miss - Checks or config changed
      → Full recompute required

   Line matching uses text content as key, allowing cache to survive line
   number shifts when lines are inserted/deleted elsewhere in the file.

   See: proserunner.storage for cache persistence."
  (:gen-class)
  (:require [proserunner.storage :as storage]))

(set! *warn-on-reflection* true)

(defn valid-result?
  "If no changes to the file, checks, or config: reuse the cached results.
  Used to determine if we need to compute new results."
  [{:keys [cached-result lines config checks]}]
  (and (storage/valid-config? cached-result config)
       (storage/valid-checks? cached-result checks)
       (storage/valid-lines? cached-result lines)))

(defn valid-checks?
  "If no changes to config or checks: the checks remain valid.
  We need only compute the changes to lines."
  [{:keys [cached-result config checks]}]
  (and (storage/valid-config? cached-result config)
       (storage/valid-checks? cached-result checks)))

(defn- text-to-line-number-map
  "Takes `lines` and returns a map of each line's text to its line number.

   Lines with duplicate text are EXCLUDED from the map to ensure correctness.

   Why? When the same text appears on multiple lines (e.g., lines 3, 5, 6 all have
   'Same text'), a simple text->line-num map can only store one line number.
   The last occurrence would overwrite earlier ones, causing cached results for
   lines 3 and 5 to incorrectly map to line 6.

   Solution: Exclude duplicates from the cache entirely. Duplicate lines will be
   treated as 'changed' and always reprocessed fresh. This trades a small
   performance cost (reprocessing duplicates) for correctness (no misattributed
   line numbers). Since duplicate lines are rare in real prose, this is acceptable."
  [lines]
  (let [text-counts (frequencies (map :text lines))]
    (reduce (fn [m {:keys [text line-num]}]
              (if (= 1 (get text-counts text))
                (assoc m text line-num)
                m))
            {}
            lines)))

(defn- update-line-number
  "Updates line numbers for cached results by looking them up in the new line map.

   Returns -1 for lines not found in the map (i.e., duplicate lines that were
   excluded from the cache). These will be filtered out by `combine`.

   Why -1 instead of nil? The Line record declares line-num as ^long (primitive),
   which cannot hold nil. Attempting to assoc nil causes the code to hang
   indefinitely. Using -1 as a sentinel value works because line numbers are
   always >= 1 in real files."
  [cached-line-map results]
  (mapv (fn [result]
          (let [text (:text result)
                line-num (get cached-line-map text -1)]  ; -1 = not found (duplicate line)
            (assoc result :line-num line-num)))
        results))

(defn- changed
  "Returns a sequence of lines that differ from the cached lines."
  [cached-line-map new-lines]
  (reduce (fn [changed-lines line]
            (if (get cached-line-map (:text line))
              changed-lines
              (conj changed-lines line)))
          []
          new-lines))

(defn- combine
  "Combines cached results with newly processed results.

   The cached results have been updated with new line numbers via `update-line-number`.
   Results with line-num = -1 are filtered out - these are cached results for
   duplicate lines that couldn't be mapped to the current file state.

   Since duplicate lines are always reprocessed (they're in `new-results`),
   filtering out the stale cached versions prevents duplicates and ensures
   we use the fresh results instead.

   Finally, `distinct` removes any remaining duplicates (e.g., if a line didn't
   change between runs)."
  [cached-results new-results]
  (->> new-results
       (concat cached-results)
       (remove #(= -1 (:line-num %)))  ; Drop cached results for duplicate lines
       distinct))

(defn compute-changed
  "Only process lines that have changed since the last vetting. If line numbers
  have shifted, update the results so they map to the correct line number."
  [input process-fn]
  (let [{:keys [file lines config checks cached-result output parallel-lines]} input
        cached-line-map (->> cached-result :lines text-to-line-number-map)
        line-num-map (text-to-line-number-map lines)
        updated-results (update-line-number line-num-map (:results cached-result))]
    (storage/->Result
     lines
     (storage/stable-hash lines)
     (storage/stable-hash file)
     config
     (storage/stable-hash config)
     (storage/checks-hash checks)
     output
     (let [changed-lines (changed cached-line-map lines)]
       (->> (process-fn checks changed-lines parallel-lines)
            (combine updated-results))))))
