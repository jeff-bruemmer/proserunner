(ns proserunner.process
  "File processing orchestration.

  This namespace contains the main file processing logic, separated from
  core.clj to avoid circular dependencies with the effects system."
  (:gen-class)
  (:require [proserunner
             [console :as console]
             [output :as output]
             [result :as result]
             [vet :as vet]]))

(set! *warn-on-reflection* true)

(def ^:private summarized-formats
  "Formats people read in a terminal; the others are for tools or documents."
  #{"group" "table"})

(defn proserunner
  "Proserunner takes options and vets the given paths with the supplied checks.
  Prints the results, then a one-line summary to stderr for terminal formats.

  Returns Result with {:issue-count :file-count}, or Failure on error."
  [options]
  (result/try-result-with-context
   (fn []
     (result/bind
      (vet/compute-paths options)
      (fn [payload]
        (let [issue-count (output/out payload)
              file-count (:file-count payload)]
          (when (summarized-formats (:output options))
            (console/status (output/summary issue-count file-count)))
          (result/ok {:issue-count issue-count
                      :file-count file-count})))))
   {:operation :process-file}))
