(ns proserunner.config.loader
  "Config file loading functions (no project dependencies)."
  (:gen-class)
  (:require [proserunner.config.types :as types]
            [proserunner.result :as result]
            [clojure.edn :as edn]))

(set! *warn-on-reflection* true)

(defn load-config-from-file
  "Loads and parses a config file, returning Result<Config>.

  This is the primary config loading function that handles all error cases:
  - File not found
  - File read errors
  - EDN parse errors

  Returns Success<Config> or Failure with error details."
  [filepath]
  (result/try-result-with-context
   (fn []
     (let [content (slurp filepath)
           parsed (edn/read-string content)
           with-defaults (if (contains? parsed :ignore)
                           parsed
                           (assoc parsed :ignore "ignore"))]
       (types/map->Config with-defaults)))
   {:filepath filepath :operation :load-config}))
