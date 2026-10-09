(ns proserunner.cache-config
  "Cache directory resolution from configuration.")

(set! *warn-on-reflection* true)

(defn resolve-cache-path
  "Determines cache directory path from configuration.

   Input map keys:
   - :cli-cache-dir    - from --cache-dir flag (highest priority)
   - :env-vars         - map of environment variables
   - :system-props     - map of system properties

   Returns string path to cache directory.

   Priority: CLI flag > PROSERUNNER_CACHE_DIR env > XDG_CACHE_HOME > tmpdir

   Examples:
     (resolve-cache-path {:cli-cache-dir \"/custom\"})
     => \"/custom\"

     (resolve-cache-path {:env-vars {\"PROSERUNNER_CACHE_DIR\" \"/env\"}})
     => \"/env\"

     (resolve-cache-path {:system-props {\"java.io.tmpdir\" \"/tmp\"}})
     => \"/tmp/proserunner-storage\""
  [{:keys [cli-cache-dir env-vars system-props]}]
  (or cli-cache-dir
      (get env-vars "PROSERUNNER_CACHE_DIR")
      (when-let [xdg (get env-vars "XDG_CACHE_HOME")]
        (str xdg "/proserunner"))
      (str (or (get env-vars "TMPDIR") (get system-props "java.io.tmpdir"))
           "/proserunner-storage")))

(defn make-cache-file-path
  "Constructs full path to a cache file.

   Input:
   - cache-dir  - base cache directory path
   - file-hash  - hash identifying the file

   Returns string path to cache file.

   Example:
     (make-cache-file-path \"/tmp/cache\" \"abc123\")
     => \"/tmp/cache/fileabc123.edn\""
  [cache-dir file-hash]
  (str cache-dir "/file" file-hash ".edn"))

(defn resolve-cache-dir-from-opts
  "Resolves cache directory from options map.

   Input: opts map (may contain :cache-dir key)
   Returns: String path to cache directory

   Priority: CLI flag > PROSERUNNER_CACHE_DIR env > XDG_CACHE_HOME > tmpdir"
  [opts]
  (resolve-cache-path
    {:cli-cache-dir (:cache-dir opts)
     :env-vars (System/getenv)
     :system-props (System/getProperties)}))

(defn cache-config
  "Extracts cache directory from options map.

   DEPRECATED: Use resolve-cache-dir-from-opts for cleaner API.
   Returns map with :cli-cache-dir for use with resolve-cache-path."
  [opts]
  {:cli-cache-dir (:cache-dir opts)})
