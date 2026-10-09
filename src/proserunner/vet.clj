(ns proserunner.vet
  "Computes results of running all the checks on each file,
   using cached results where possible."
  (:gen-class)
  (:require
   [proserunner
    [storage :as store]
    [result :as result]
    [text :as text]]
   [proserunner.vet
    [cache :as cache]
    [processor :as processor]
    [input :as input]]
   [editors
    [re :as re]
    [repetition :as repetition]
    [registry :as registry]
    [utilities :as util]]))

(set! *warn-on-reflection* true)

;;; Register all standard editors

(doseq [editor-type (util/standard-editor-types)]
  (registry/register-editor! editor-type (util/create-editor editor-type)))

(registry/register-editor! "repetition" repetition/proofread)
(registry/register-editor! "regex" re/proofread)

;;; Core computation

(defn compute
  "Takes an input, and returns the results of
  running the configured checks on each line of text in the file."
  [{:keys [file lines config checks output parallel-lines]}]
  (store/map->Result {:lines lines
                      :lines-hash (store/stable-hash lines)
                      :file-hash (store/stable-hash file)
                      :config config
                      :config-hash (store/stable-hash config)
                      :check-hash (store/stable-hash checks)
                      :output output
                      :results (processor/process checks lines parallel-lines)}))

(defn- compute-and-store
  [inputs options]
  (let [result (compute inputs)]
    (store/save! result options)
    result))

(defn compute-or-cached
  "Returns computed or cached results of running checks on text.

  Returns Result with computed/cached results, or Failure on error."
  [options]
  (let [input-result (input/make options)]
    (if (result/failure? input-result)
      input-result
      (let [inputs (:value input-result)
            {:keys [cached-result output]} inputs
            results
            (cond
              ;; Standard input has no path to cache it under
              (text/stdin? (:file options))
              (compute inputs)

              (:no-cache inputs)
              (compute-and-store inputs options)

              (cache/valid-result? inputs)
              (assoc cached-result :output output)

              (cache/valid-checks? inputs)
              (let [result (cache/compute-changed inputs processor/process)]
                (store/save! result options)
                result)

              :else
              (compute-and-store inputs options))]
        (result/ok (assoc inputs :results results))))))

(defn- merge-payloads
  "Folds one path's payload into the accumulated payload: issues are
  concatenated, file counts summed, everything else kept from the first."
  [acc payload]
  (if (nil? acc)
    payload
    (-> acc
        (update-in [:results :results]
                   #(vec (concat % (get-in payload [:results :results]))))
        (update :file-count + (:file-count payload)))))

(defn compute-paths
  "Runs compute-or-cached on each path in (:paths options), or on (:file options)
  when there are no paths, and merges the results into a single payload so issue
  numbers span the whole run. Each path keeps its own cache entry.
  The payload gains :file-count, the number of files checked.

  Returns Result with the merged payload, or the first Failure."
  [{:keys [paths file] :as options}]
  (reduce (fn [acc path]
            (let [r (compute-or-cached (assoc options :file path))]
              (if (result/failure? r)
                (reduced r)
                (result/ok
                 (merge-payloads (result/get-value acc)
                                 (assoc (:value r) :file-count
                                        (input/count-files path options)))))))
          (result/ok nil)
          (if (seq paths) paths [file])))
