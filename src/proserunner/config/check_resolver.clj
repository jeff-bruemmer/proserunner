(ns proserunner.config.check-resolver
  "Resolves check entries (strings/maps) to check definitions."
  (:gen-class)
  (:require [clojure.java.io :as io]
            [clojure.set :as set]
            [clojure.string :as string]
            [proserunner.config.manifest :as manifest]
            [proserunner.file-utils :as file-utils]
            [proserunner.system :as sys])
  (:import java.io.File))

(set! *warn-on-reflection* true)

(defn- relative-path?
  "Checks if path is relative."
  [path]
  (not (.isAbsolute (io/file path))))

(defn get-edn-files
  "Gets all .edn files in a directory, returning their names without extension."
  [directory]
  (let [dir-file (io/file directory)]
    (when (.exists dir-file)
      (->> (.listFiles dir-file)
           (filter #(.isFile ^File %))
           (map #(.getName ^File %))
           (filter #(string/ends-with? % ".edn"))
           (map #(string/replace % #"\.edn$" ""))
           sort
           vec))))

(defn- file-not-found-error
  "Builds error for missing check source files."
  [source resolved-path]
  (str "Local check source does not exist: " source
       "\nResolved to: " resolved-path
       "\nPlease verify the path is correct."))

(defn- resolve-local-path
  "Resolves a local path to absolute path, validating it exists and is safe.
   Ensures the resolved path is within project-root to prevent directory traversal."
  [source project-root]
  (let [source-file (if (relative-path? source)
                      (io/file project-root source)
                      (io/file source))
        canonical-path (.getCanonicalPath source-file)]

    (when project-root
      (let [canonical-root (.getCanonicalPath (io/file project-root))]
        (when-not (.startsWith canonical-path canonical-root)
          (throw (ex-info "Path traversal detected: path outside project root"
                         {:source source
                          :resolved-path canonical-path
                          :project-root canonical-root
                          :type :path-traversal})))))

    (when-not (.exists source-file)
      (throw (ex-info (file-not-found-error source canonical-path)
                     {:source source
                      :resolved-path canonical-path
                      :type :file-not-found})))

    canonical-path))

(defn- make-directory-absolute
  "Converts a directory path to absolute, relative to base-dir if needed."
  [dir base-dir]
  (if (file-utils/absolute-path? dir)
    dir
    (str (io/file base-dir dir))))

(defn- find-global-check-by-directory
  "Finds a global check entry that matches the given directory name.
   Returns the check entry with absolute paths, or nil if not found."
  [dir-name global-checks]
  ;; Empty string gets us the global config directory, with a trailing separator
  (let [global-proserunner-dir (sys/config-path "")]
    (when-let [check (first (filter #(= (:directory %) dir-name) global-checks))]
      (update check :directory #(make-directory-absolute % global-proserunner-dir)))))

(defn- generate-check-name
  "Generates a check name from a directory path."
  [directory]
  (str "Project checks: " (last (string/split directory (re-pattern (str File/separator))))))

(defn- resolve-check-directory
  "Resolves a check directory path, handling the special 'checks' shorthand."
  [directory project-root]
  (if (= directory "checks")
    (manifest/project-checks-dir project-root)
    (resolve-local-path directory project-root)))

(defn- normalize-project-check-entry
  "Normalizes a project check map entry.
   - Resolves :directory to absolute path
   - Auto-discovers :files if not present
   - Generates :name if not present"
  [check-entry project-root]
  (let [directory (resolve-check-directory (:directory check-entry) project-root)
        files (if (contains? check-entry :files)
               (:files check-entry)
               (get-edn-files directory))
        name (or (:name check-entry)
                (generate-check-name directory))]
    (when (seq files)
      {:name name
       :directory directory
       :files files})))

(defn- resolve-check-entry
  "Resolves a check entry (string reference or map) to concrete check config.
   - String: looks up in global-checks by directory name
   - Map: normalizes and resolves paths
   Returns check config map or nil if resolution fails."
  [entry global-checks project-root]
  (if (string? entry)
    (find-global-check-by-directory entry global-checks)
    (normalize-project-check-entry entry project-root)))

(defn resolve-check-entries
  "Resolves all check entries from a project config into check definitions.
   Returns a vector of check configs ready for the checks system.

   Entries that resolve to nil are filtered out (e.g., empty directories)."
  [check-entries global-checks project-root]
  (->> check-entries
       (map #(resolve-check-entry % global-checks project-root))
       (remove nil?)
       vec))

(defn missing-global-check-directories
  "Returns set of missing global check directory names referenced by string entries.
   Only checks for 'default' and 'custom' as these are downloadable.
   Map entries (project-local checks) are ignored."
  [check-entries global-checks]
  (let [downloadable #{"default" "custom"}
        string-refs (->> check-entries
                        (filter string?)
                        (filter downloadable)
                        set)
        existing-dirs (->> global-checks
                          (map :directory)
                          set)]
    (set/difference string-refs existing-dirs)))
