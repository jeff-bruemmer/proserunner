(ns proserunner.project-config
  "Functions for loading and managing project-level configuration."
  (:refer-clojure :exclude [load read])
  (:require [clojure.java.io :as io]
            [clojure.pprint :as pprint]
            [proserunner.config.loader :as loader]
            [proserunner.config.manifest :as manifest]
            [proserunner.config.merger :as merger]
            [proserunner.config.check-resolver :as check-resolver]
            [proserunner.edn-utils :as edn-utils]
            [proserunner.file-utils :as file-utils]
            [proserunner.ignore.file :as ignore-file]
            [proserunner.result :as result]
            [proserunner.system :as sys]))

(set! *warn-on-reflection* true)

;;; Error Message Builders

(defn- no-project-config-error
  "Builds error when --project flag used but no project exists."
  [start-dir]
  (str "No project configuration found"
       "\nSearched from: " start-dir
       "\nTo create a project configuration, run: proserunner --init-project"))

(defn- conflicting-flags-error
  "Builds error for conflicting --global and --project flags."
  []
  "Cannot specify both --global and --project flags. Choose one.")

;;; Global Config Loading

(defn- normalize-check-entry
  "Adds :files to check entry if missing by auto-discovering .edn files.
   For global config, resolves directory relative to ~/.proserunner/"
  [global-proserunner-dir check-entry]
  (if (contains? check-entry :files)
    check-entry
    (let [dir-path (io/file global-proserunner-dir (:directory check-entry))
          files (check-resolver/get-edn-files (.getAbsolutePath dir-path))]
      (assoc check-entry :files (or files [])))))

(defn load-global-config
  "Loads the global configuration from ~/.proserunner/"
  []
  (let [config-path (sys/filepath ".proserunner" "config.edn")
        config-file (io/file config-path)]
    (if (.exists config-file)
      (let [config-result (loader/load-config-from-file config-path)]
        (if (result/success? config-result)
          (let [parsed-config-map (:value config-result)
                global-proserunner-dir (sys/filepath ".proserunner" "")
                normalized-checks (mapv #(normalize-check-entry global-proserunner-dir %)
                                       (:checks parsed-config-map))
                {:keys [ignore ignore-issues]} (ignore-file/read)]
            {:checks normalized-checks
             :ignore ignore
             :ignore-issues ignore-issues})
          ;; If config load fails, return empty config (graceful degradation)
          {:checks [] :ignore #{} :ignore-issues #{}}))
      {:ignore #{} :ignore-issues #{}})))

(defn- build-project-config
  "Builds final project config from merged config and resolved checks."
  [merged-config global-config project-root]
  (let [check-entries (:checks merged-config)
        ;; Resolve all check entries (string references and map definitions)
        resolved-checks (check-resolver/resolve-check-entries check-entries (:checks global-config) project-root)]
    {:checks resolved-checks
     :ignore (:ignore merged-config)
     :ignore-issues (:ignore-issues merged-config)
     :source :project}))

(defn load
  "Main entry point for loading project configuration.

   Orchestrates the entire config merge pipeline (see 'Config Merging' section above):

   1. Searches for .proserunner/config.edn starting from start-dir (skips home directory)
   2. If found, parses and validates it (parse-manifest)
   3. Loads global config from ~/.proserunner/ (load-global-config)
   4. Merges global and project configs based on project settings (merge-configs)
   5. Resolves check entries into check definitions (resolve-check-entries)
   6. Builds final configuration with resolved checks (build-project-config)
   7. Returns the final configuration with :checks, :ignore, and :source

   If no project manifest is found, returns global config with :source :global.

   See the 'Config Merging' section documentation for detailed examples and flow diagrams."
  [start-dir]
  (if-let [{:keys [manifest-path project-root]} (manifest/find start-dir)]
    (let [project-config (manifest/read manifest-path)
          global-config (load-global-config)
          merged (merger/merge-configs global-config project-config)]
      (build-project-config merged global-config project-root))
    (assoc (load-global-config) :source :global)))

;;; Context-Aware Target Determination

(defn resolve-start-dir
  "Resolves the start directory from options or current working directory.
   Public helper to provide consistent start-dir resolution."
  [options]
  (or (:start-dir options) (System/getProperty "user.dir")))

(defn determine-target
  "Determines whether to add checks/ignores to :global or :project scope.

   Options map may contain:
   - :global - Force global scope (~/.proserunner/)
   - :project - Force project scope (.proserunner/), fails if no project
   - neither - Auto-detect based on presence of .proserunner/ in start-dir

   Returns:
   - :global - Add to ~/.proserunner/
   - :project - Add to .proserunner/ in project root

   Throws:
   - ExceptionInfo if both :global and :project specified
   - ExceptionInfo if :project specified but no project found"
  [options start-dir]
  (cond
    ;; Both flags specified is an error
    (and (:global options) (:project options))
    (throw (ex-info (conflicting-flags-error)
                    {:options options
                     :type :conflicting-flags}))

    ;; Explicit global
    (:global options)
    :global

    ;; Explicit project - verify it exists
    (:project options)
    (if (manifest/find start-dir)
      :project
      (throw (ex-info (no-project-config-error start-dir)
                      {:start-dir start-dir
                       :type :no-project-found})))

    ;; Auto-detect: if project exists, use it; otherwise use global
    :else
    (if (manifest/find start-dir)
      :project
      :global)))

(defn read
  "Read the project .proserunner/config.edn file.
   Returns the parsed config map, or nil if file doesn't exist."
  [project-root]
  (let [config-path (manifest/project-config-path project-root)]
    (when (.exists (io/file config-path))
      (let [result (edn-utils/read-edn-file config-path)]
        (when (result/success? result)
          (:value result))))))

(defn write!
  "Write config map to project .proserunner/config.edn file atomically."
  [project-root config]
  (let [config-path (manifest/project-config-path project-root)]
    (file-utils/atomic-spit config-path (with-out-str (pprint/pprint config)))))

;;; Project Initialization

(def default-manifest-template
  "Default template for a new project manifest."
  {:checks ["default"]
   :ignore #{}
   :ignore-issues #{}
   :ignore-mode :extend
   :config-mode :merged})

(defn init!
  "Creates a .proserunner/ directory structure in the specified directory.
   Creates .proserunner/config.edn and .proserunner/checks/ subdirectory.
   Returns the path to the created config file, or throws if file already exists."
  ([dir] (init! dir default-manifest-template))
  ([dir manifest-template]
   (when (manifest/exists? dir)
     (throw (ex-info "Project manifest already exists"
                     {:path (.getAbsolutePath (manifest/path dir))})))
   (let [checks-dir (manifest/project-checks-dir-path dir)
         config-file (manifest/path dir)]
     (file-utils/mkdirs-if-missing (.getAbsolutePath checks-dir))
     (file-utils/atomic-spit (.getAbsolutePath config-file)
                             (str ";; Proserunner Project Configuration\n"
                                  ";; See: https://github.com/jeff-bruemmer/proserunner\n\n"
                                  (with-out-str (pprint/pprint manifest-template))))
     (.getAbsolutePath config-file))))
