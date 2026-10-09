(ns proserunner.custom-checks
  "Functions for adding custom checks from external sources."
  (:gen-class)
  (:require [proserunner.console :as console]
            [proserunner.context :as context]
            [proserunner.edn-utils :as edn-utils]
            [proserunner.project-config :as project-config]
            [proserunner.config.manifest :as manifest]
            [proserunner.file-utils :as file-utils]
            [proserunner.result :as result]
            [proserunner.system :as sys]
            [clojure.java.io :as io]
            [clojure.string :as string]
            [clojure.pprint :as pprint])
  (:import java.io.File))

(set! *warn-on-reflection* true)

;;; Error Message Builders

(defn- no-checks-found-error
  "Builds error data for when no .edn files found in source directory."
  [source-dir]
  {:message (str "No .edn check files found in directory: " source-dir)
   :source source-dir
   :type :no-checks-found
   :help ["Expected: Files ending with .edn containing check definitions"
          "See: docs/checks.md for check file format"]})

(defn- throw-no-checks-found!
  "Throws formatted error when no checks found."
  [source-dir]
  (let [err (no-checks-found-error source-dir)
        help-text (str "\n\n" (string/join "\n" (:help err)))]
    (throw (ex-info (str (:message err) help-text)
                    (dissoc err :message :help)))))

(defn- extract-name-from-source
  "Extract a reasonable name from the source path."
  [source]
  (let [cleaned (string/replace source #"/$" "")
        parts (string/split cleaned #"[/\\]")]
    (last parts)))

(defn- find-edn-files
  "Find all .edn files in a directory recursively."
  [dir-path]
  (let [dir (io/file dir-path)]
    (when (.exists dir)
      (->> (file-seq dir)
           (filter #(.isFile ^File %))
           (filter #(string/ends-with? (.getName ^File %) ".edn"))
           (map #(.getAbsolutePath ^File %))))))

(defn- copy-file!
  "Copy a file to destination, creating parent directories if needed."
  [^File src-file dest-path]
  (let [dest (io/file dest-path)]
    (io/make-parents dest)
    (io/copy src-file dest)))

(defn- copy-checks-to-directory
  "Copy .edn files from source-dir to target-dir.
   Returns map with :target-dir, :check-names, and :count."
  [source-dir target-dir]
  (let [edn-files (find-edn-files source-dir)]
    (when (empty? edn-files)
      (throw-no-checks-found! source-dir))

    ;; Create target directory
    (file-utils/mkdirs-if-missing target-dir)

    ;; Copy each .edn file
    (doseq [file-path edn-files]
      (let [file (io/file file-path)
            filename (.getName file)
            dest-path (file-utils/join-path target-dir filename)]
        (copy-file! file dest-path)))

    {:target-dir target-dir
     :check-names (map #(string/replace (.getName (io/file %)) #"\.edn$" "") edn-files)
     :count (count edn-files)}))

(defn- add-checks-from-directory
  "Copy .edn files from a local directory to global custom checks directory."
  [source-dir target-name]
  (let [target-dir (sys/config-path "custom" target-name)]
    (copy-checks-to-directory source-dir target-dir)))

(defn- add-checks-to-project-dir
  "Copy .edn files from a local directory to project .proserunner/checks/ directory."
  [source-dir _target-name project-root]
  (let [target-dir (manifest/project-checks-dir project-root)]
    (copy-checks-to-directory source-dir target-dir)))

(defn- read-config
  "Read the config.edn file."
  []
  (let [config-path (sys/config-path "config.edn")]
    (when (.exists (io/file config-path))
      (let [result (edn-utils/read-edn-file config-path)]
        (when (result/success? result)
          (:value result))))))

(defn- write-config!
  "Write config map to config.edn file atomically."
  [config]
  (let [config-path (sys/config-path "config.edn")]
    (file-utils/atomic-spit config-path (with-out-str (pprint/pprint config)))))

(defn- update-config-with-checks!
  "Add new check entry to config.edn."
  [target-name check-names]
  (let [config (or (read-config) {:checks []})
        custom-dir-name target-name
        existing-checks (:checks config)

        ;; Check if this directory already exists in config
        existing-entry (first (filter #(= (:directory %) (str "custom/" custom-dir-name))
                                      existing-checks))

        new-entry {:name (str "Custom checks: " target-name)
                   :directory (str "custom/" custom-dir-name)
                   :files (vec check-names)}

        updated-checks (if existing-entry
                        ;; Replace existing entry
                        (mapv #(if (= (:directory %) (str "custom/" custom-dir-name))
                                new-entry
                                %)
                             existing-checks)
                        ;; Add new entry
                        (conj (vec existing-checks) new-entry))]

    (write-config! (assoc config :checks updated-checks))))

(defn- update-project-config-with-checks!
  "Add checks directory reference to :checks in project config if not already present."
  [project-root]
  (let [config (project-config/read project-root)
        checks (:checks config)
        has-checks? (some manifest/check-entry-references-checks? checks)
        updated-checks (if has-checks?
                        checks
                        (conj (vec checks) {:directory "checks"}))]
    (project-config/write! project-root (assoc config :checks updated-checks))))

(defn- import-from-source
  "Import checks from local directory.
   Returns result map with :target-dir, :check-names, and :count."
  [source target-name project-root]
  (console/status "Importing from " source "...")
  (if project-root
    (add-checks-to-project-dir source target-name project-root)
    (add-checks-from-directory source target-name)))

(defn- print-success-message
  "Print success message for added checks."
  [target result config-path]
  (let [scope-name (if (= target :global) "global config" "project")
        alt-flag (if (= target :global) "--project" "--global")
        alt-scope (if (= target :global) "project" "global")
        extra-msg (if (= target :project)
                   "\nThese checks will apply to this project only."
                   "")]
    (console/status "\nAdded " (:count result) " checks to " scope-name)
    (console/status "  + " (:target-dir result))
    (console/status "  + Updated config: " config-path)
    (console/status "\nChecks added: " (string/join ", " (:check-names result)))
    (console/status extra-msg)
    (console/status "Use " alt-flag " to add to " alt-scope " instead.")))

(defn add-checks
  "Add checks from a local directory with context-aware targeting.

   Options:
   - :name - Custom name for the check directory (optional, defaults to source basename)
   - :global - Force global scope (custom/ in the global config directory)
   - :project - Force project scope (.proserunner/checks/), fails if no project
   - :start-dir - Starting directory for project detection (defaults to user.dir)"
  [source options]
  (let [target-name (or (:name options) (extract-name-from-source source))]
    (context/with-context options
      (fn [{:keys [target project-root]}]
        (sys/call-with-config-lock
         #(if (= target :global)
            ;; Global scope
            (let [result (import-from-source source target-name nil)
                  config-path (sys/config-path "config.edn")]
              (update-config-with-checks! target-name (:check-names result))
              (print-success-message target result config-path)
              (assoc result :target :global))

            ;; Project scope
            (let [result (import-from-source source target-name project-root)
                  config-path (manifest/project-config-path project-root)]
              (update-project-config-with-checks! project-root)
              (print-success-message target result config-path)
              (assoc result :target :project))))))))
