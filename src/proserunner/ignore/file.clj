(ns proserunner.ignore.file
  "File I/O operations for ignore lists."
  (:refer-clojure :exclude [read])
  (:gen-class)
  (:require [clojure.java.io :as io]
            [clojure.pprint :as pprint]
            [proserunner.edn-utils :as edn-utils]
            [proserunner.file-utils :as file-utils]
            [proserunner.result :as result]
            [proserunner.system :as sys]))

(set! *warn-on-reflection* true)

;;; Global Ignore File Management

(defn path
  "Returns the path to the ignore file."
  []
  (sys/config-path "ignore.edn"))

(defn read
  "Reads the ignore file and returns a map with :ignore (set) and :ignore-issues (set).
   Handles legacy vector format by converting to set."
  []
  (let [ignore-path (path)]
    (if (.exists (io/file ignore-path))
      (let [result (edn-utils/read-edn-file ignore-path)]
        (if (result/success? result)
          (let [data (:value result)
                ignore-issues (:ignore-issues data)]
            {:ignore (or (:ignore data) #{})
             :ignore-issues (if (vector? ignore-issues)
                              (set ignore-issues)
                              (or ignore-issues #{}))})
          {:ignore #{} :ignore-issues #{}}))
      {:ignore #{} :ignore-issues #{}})))

(defn write!
  "Writes ignore map to the ignore file atomically.
   Takes map with :ignore (set of strings) and :ignore-issues (set of maps).
   Converts ignore-issues to sorted vector for human-readable output."
  [{:keys [ignore ignore-issues]}]
  (let [ignore-path (path)
        sorted-simple (set (sort ignore))
        sorted-contextual (vec (sort-by (fn [entry]
                                          [(:file entry)
                                           (or (:line-num entry) (:line entry) 0)
                                           (:specimen entry)])
                                        ignore-issues))
        output-map {:ignore sorted-simple
                    :ignore-issues sorted-contextual}]
    (file-utils/ensure-parent-dir ignore-path)
    (file-utils/atomic-spit ignore-path (with-out-str (pprint/pprint output-map)))))
