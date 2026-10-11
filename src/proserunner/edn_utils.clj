(ns proserunner.edn-utils
  "Utilities for reading and parsing EDN files with Result-based error handling."
  (:require [clojure.edn :as edn]
            [proserunner.result :as result])
  (:gen-class))

(set! *warn-on-reflection* true)

(defn read-edn-file
  "Reads and parses an EDN file, returning Result<data>.

  Returns Success with parsed EDN data, or Failure with error details.
  Handles file not found, read errors, and parse errors.

  Example:
    (read-edn-file \"config.edn\")
    ;; => #proserunner.result.Success{:value {:foo \"bar\"}}

    (read-edn-file \"missing.edn\")
    ;; => #proserunner.result.Failure{:error \"File not found\" ...}"
  [filepath]
  (result/try-result-with-context
   (fn []
     (let [content (slurp filepath)]
       (edn/read-string content)))
   {:filepath filepath :operation :read-edn-file}))

(defn read-edn-string
  "Parses an EDN string, returning Result<data>.

  Returns Success with parsed EDN data, or Failure on parse errors.

  Example:
    (read-edn-string \"{:foo \\\"bar\\\"}\")
    ;; => #proserunner.result.Success{:value {:foo \"bar\"}}

    (read-edn-string \"{:invalid edn\")"
  [edn-str]
  (result/try-result-with-context
   (fn []
     (edn/read-string edn-str))
   {:operation :read-edn-string}))

(defn read-edn-file-with-readers
  "Reads and parses an EDN file with custom readers, returning Result<data>.

  readers-opts should be a map suitable for edn/read-string, e.g.:
    {:readers {'my.ns.Record map->Record}}

  Returns Success with parsed EDN data, or Failure with error details.

  Example:
    (read-edn-file-with-readers \"data.edn\" {:readers {'my.Record map->Record}})
    ;; => #proserunner.result.Success{:value #my.Record{:field \"value\"}}"
  [filepath readers-opts]
  (result/try-result-with-context
   (fn []
     (let [content (slurp filepath)]
       (edn/read-string readers-opts content)))
   {:filepath filepath :operation :read-edn-file-with-readers}))
