(ns proserunner.config.manifest
  "Project manifest discovery and parsing."
  (:gen-class)
  (:refer-clojure :exclude [read find])
  (:require [clojure.java.io :as io]
            [proserunner.edn-utils :as edn-utils]
            [proserunner.file-utils :as file-utils]
            [proserunner.result :as result]
            [proserunner.system :as sys]
            [proserunner.validation :as v])
  (:import java.io.File))

(set! *warn-on-reflection* true)

(def ^:private proserunner-dir-name ".proserunner")
(def ^:private config-file-name "config.edn")
(def ^:private checks-dir-name "checks")

(defn path
  "Returns path to manifest file within a directory."
  ^File [dir]
  (io/file dir proserunner-dir-name config-file-name))

(defn project-checks-dir-path
  "Returns path to .proserunner/checks/ directory within project root."
  ^File [project-root]
  (io/file project-root proserunner-dir-name checks-dir-name))

(defn- has-git-directory?
  "Checks if the given directory contains a .git folder."
  [^File dir]
  (.exists (io/file dir ".git")))

(defn- manifest-in-dir
  "Returns map with :manifest-path and :project-root if manifest exists in dir, nil otherwise."
  [^File dir]
  (let [manifest-file (path dir)]
    (when (.exists manifest-file)
      {:manifest-path (.getAbsolutePath manifest-file)
       :project-root (.getAbsolutePath dir)})))

(defn find
  "Walks up the directory tree from start-dir to find .proserunner/config.edn.
   Stops at the first .git directory encountered (project boundary).
   Skips the home directory, where ~/.proserunner was the global config before XDG.
   Returns map with :manifest-path and :project-root, or nil if not found."
  [start-dir]
  (let [home-dir (sys/home-dir)]
    (loop [current-dir (io/file start-dir)]
      (when (and current-dir
                 (not= (.getAbsolutePath current-dir) home-dir))
        (or (manifest-in-dir current-dir)
            (when-not (has-git-directory? current-dir)
              (recur (.getParentFile current-dir))))))))

(defn exists?
  "Checks if a .proserunner/config.edn file exists in the given directory."
  [dir]
  (.exists (path dir)))

(defn check-entry-references-checks?
  "Returns true if entry references the 'checks' directory (string 'checks' or map with :directory 'checks')."
  [entry]
  (or (= entry "checks")
      (and (map? entry) (= (:directory entry) "checks"))))

(defn- valid-check-entry?
  "Validates a single check entry (string reference or map definition)."
  [entry]
  (or (string? entry)
      (and (map? entry)
           (contains? entry :directory)
           (string? (:directory entry))
           (or (nil? (:files entry))
               (and (vector? (:files entry))
                    (every? string? (:files entry))))
           (or (nil? (:name entry))
               (string? (:name entry))))))

(defn- validate-entries-format
  "Validates that all entries in a check list are valid."
  [field-name]
  (fn [value]
    (let [invalid-entries (remove valid-check-entry? value)]
      (if (seq invalid-entries)
        (v/validation-error value
          (str field-name " contains invalid entries. Each entry must be either:\n"
               "  - A string (reference to global check)\n"
               "  - A map with :directory (required), and optional :files (vector) and :name (string)\n"
               "Invalid entries: " (pr-str invalid-entries)))
        (v/validation-ok value)))))

(defn- validate-check-entries
  "Custom validator for :checks field that validates all entries."
  [field-name]
  (v/chain
    (v/required field-name)
    (v/type-check field-name vector? "a vector")
    (v/not-empty-check field-name)
    (validate-entries-format field-name)))

(def ^:private manifest-validators
  "Declarative validator specifications for project manifest fields."
  {:checks
   (validate-check-entries ":checks")

   :ignore
   (v/with-default #{}
     (v/type-check ":ignore" set? "a set"))

   :ignore-issues
   (v/with-default #{}
     (v/type-check ":ignore-issues" set? "a set"))

   :ignore-mode
   (v/with-default :extend
     (v/enum-check ":ignore-mode" #{:extend :replace}))

   :config-mode
   (v/with-default :merged
     (v/enum-check ":config-mode" #{:merged :project-only}))})

(defn parse
  "Parses and validates a project manifest map.
   Throws with ALL validation errors if invalid.
   Returns validated map with defaults applied if valid."
  [manifest]
  (v/validate! manifest-validators manifest))

(defn read
  "Reads and parses a manifest file from the given path."
  [manifest-path]
  (let [read-result (edn-utils/read-edn-file manifest-path)]
    (if (result/success? read-result)
      (parse (:value read-result))
      (throw (ex-info (str "Failed to read manifest file: " manifest-path)
                      {:path manifest-path
                       :error (:error read-result)
                       :context (:context read-result)})))))

(defn project-config-path
  "Returns path string to project config.edn file."
  ^String [project-root]
  (file-utils/join-path project-root proserunner-dir-name config-file-name))

(defn project-checks-dir
  "Returns path string to project .proserunner/checks directory."
  ^String [project-root]
  (file-utils/join-path project-root proserunner-dir-name "checks"))
