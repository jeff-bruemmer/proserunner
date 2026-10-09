(ns proserunner.vet.processor
  "Core check processing engine for applying checks to lines of text.

   Processing flow:
   1. For each line, apply all checks sequentially via reduce
   2. Each check dispatches to its registered editor (see editors.registry)
   3. Editors return the line with any issues attached
   4. Filter to only lines where issues were found

   Parallel processing:
   - When parallel? is true, uses pmap to process lines concurrently
   - Each line processed independently (no shared state)
   - Good for large files with many lines

   Error handling:
   - safe-dispatch catches editor errors and logs warnings
   - Failed checks don't break the entire vetting process
   - Line is returned unchanged if editor throws exception

   See: editors.registry for editor registration and dispatch mechanism."
  (:gen-class)
  (:require [editors.registry :as registry]
            [proserunner.console :as console]))

(set! *warn-on-reflection* true)

(defn safe-dispatch
  "Safely dispatch to an editor, catching and logging any errors.
   Returns the line unchanged if an error occurs."
  [line check]
  (try
    (registry/dispatch line check)
    (catch Exception e
      (let [{:keys [name kind]} check
            {:keys [file line-num]} line]
        (console/warn "Check '" name "' (" kind ") failed on "
                      file ":" line-num ": " (.getMessage e)))
      line)))

(defn dispatch
  "Takes a `line` and a `check` returns result of relevant editor.
  Uses dynamic registry for extensibility."
  [line check]
  (safe-dispatch line check))

(defn process
  "Takes `checks` and `lines` and runs each check on each line,
  return lines with any issues found.
  Uses pmap for parallel processing of lines when parallel? is true."
  [checks lines parallel?]
  (let [map-fn (if parallel? pmap map)]
    (->> lines
         (map-fn #(reduce dispatch % checks))
         (filter :issue?))))
