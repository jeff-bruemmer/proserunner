(ns proserunner.storage
  "Functions used for storing and retrieving cached results."
  (:gen-class)
  (:require [proserunner
             [cache-config :as cache-config]
             [checks :as checks]
             [edn-utils :as edn-utils]
             [file-utils :as file-utils]
             [result :as result]
             [text :as text]]
            [proserunner.config.types :as types]
            [clojure.java.io :as io]))

(set! *warn-on-reflection* true)

(defrecord Result
  [lines                      ; Vector of Line records that were vetted
   lines-hash                 ; stable-hash of lines, for cache invalidation
   file-hash                  ; stable-hash of the file path; names the cache file
   config                     ; Config record used for vetting (for reference)
   config-hash                ; stable-hash of config, for cache invalidation
   check-hash                 ; stable-hash of checks, for cache invalidation
   ^String output             ; Output format specification ("group", "table", "json", "edn", "verbose")
   results])                  ; Vector of Line records with issues found (only lines with :issue? true)

(defn resolve-cache-dir
  "Resolves cache directory from options.

   Delegates to cache-config for all resolution logic."
  [opts]
  (cache-config/resolve-cache-dir-from-opts opts))

(defn mk-cache-dir!
  "Creates cache directory if needed.

   Returns Result with cache directory path string."
  [opts]
  (result/try-result-with-context
    (fn []
      (let [cache-dir (resolve-cache-dir opts)
            p (io/file cache-dir)]
        (when-not (.exists p)
          (when-not (.mkdirs p)
            (throw (ex-info (str "Failed to create cache directory: " cache-dir)
                           {:path cache-dir
                            :exists (.exists p)
                            :writable (when (.exists (.getParentFile p))
                                       (.canWrite (.getParentFile p)))}))))
        cache-dir))
    {:operation :mk-cache-dir}))

(defn save!
  "Creates cache directory and writes result to cache file.

   Takes result (Result record) and opts map (may contain :cache-dir).
   Returns Result<Path> - Success with path to cached file, or Failure."
  [result opts]
  (result/bind
    (mk-cache-dir! opts)
    (fn [cache-dir]
      (result/try-result-with-context
        (fn []
          (let [cache-file (cache-config/make-cache-file-path
                             cache-dir
                             (:file-hash result))]
            (file-utils/atomic-spit cache-file (pr-str result))
            cache-file))
        {:operation :save-cache
         :file-hash (:file-hash result)}))))

(def ^:private edn-readers
  {:readers (merge default-data-readers
                   {'proserunner.storage.Result map->Result
                    'proserunner.text.Line text/map->Line
                    'proserunner.text.Issue text/map->Issue
                    'proserunner.checks.Check checks/map->Check
                    ;; Support both old and new Config locations for backward compatibility
                    'proserunner.config.Config types/map->Config
                    'proserunner.config.types.Config types/map->Config
                    'proserunner.checks.Recommendation checks/map->Recommendation
                    'proserunner.checks.Expression checks/map->Expression})})

(defn stable-hash
  "Computes a 32-bit hash of data's printed form. It's stable across JVM
  restarts because Java specifies String hashes. Not a cryptographic hash."
  [data]
  (hash (pr-str data)))

(defn- handle-corrupted-cache-file
  "Attempts to delete corrupted cache file.

   Arguments:
   - cache-file: File object for the corrupted cache
   - file-path: String path to cache file
   - read-error: Result containing the read error details

   Returns map with:
   - :type :corrupted-cache
   - :file cache-file-path
   - :deleted? boolean indicating if deletion succeeded
   - :original-error error from read attempt"
  [^java.io.File cache-file file-path read-error]
  (let [deleted? (try
                   (.delete cache-file)
                   true
                   (catch Exception _
                     false))]
    {:type :corrupted-cache
     :file file-path
     :deleted? deleted?
     :original-error (:error read-error)}))

(defn get-cached-result
  "Retrieves cached result for file if exists and valid.

   Takes file path and opts map (may contain :cache-dir).
   Returns Result:
   - Success with cached record if valid cache exists
   - Failure with :cache-miss if no cache file
   - Failure with :corrupted-cache (includes :deleted? status) if unreadable"
  [file opts]
  (let [cache-dir (resolve-cache-dir opts)
        cache-file-path (cache-config/make-cache-file-path
                          cache-dir
                          (stable-hash file))
        cache-file (io/file cache-file-path)]
    (if-not (.exists cache-file)
      (result/err "No cache" {:type :cache-miss})
      (let [read-result (edn-utils/read-edn-file-with-readers cache-file-path edn-readers)]
        (if (result/success? read-result)
          (result/ok (:value read-result))
          (result/err "Corrupted cache"
                     (handle-corrupted-cache-file cache-file cache-file-path read-result)))))))

;;;; Predicate functions for validating cached results by comparing hashes.

(defn valid-checks?
  "Validates that cached checks match current checks by comparing hashes.
  Returns true if the check hash in cached-result matches the hash of checks."
  [cached-result checks]
  (=
   (:check-hash cached-result)
   (stable-hash checks)))

(defn valid-lines?
  "Validates that cached lines match current lines by comparing hashes.
  Returns true if the lines hash in cached-result matches the hash of lines."
  [cached-result lines]
  (=
   (:lines-hash cached-result)
   (stable-hash lines)))

(defn valid-config?
  "Validates that cached config matches current config by comparing hashes.
  Returns true if the config hash in cached-result matches the hash of config."
  [cached-result config]
  (=
   (:config-hash cached-result)
   (stable-hash config)))
