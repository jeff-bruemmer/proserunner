(ns proserunner.config
  "Functions for creating a configuration directory."
  (:gen-class)
  (:require [proserunner
             [console :as console]
             [file-utils :as file-utils]
             [result :as result]
             [system :as sys]]
            [proserunner.config.loader :as loader]
            [proserunner.config.manifest :as manifest]
            [proserunner.config.merger :as merger]
            [proserunner.config.types :refer [map->Config]]
            [proserunner.config.check-resolver :as check-resolver]
            [proserunner.project-config :as project-config]
            [babashka.http-client :as client]
            [clojure.string :as string]
            [clojure.java.io :as io]
            [clojure.pprint :as pprint]
            [cheshire.core :as json]))

(set! *warn-on-reflection* true)

(def remote-address "https://github.com/jeff-bruemmer/proserunner-default-checks/archive/main.zip")
(def remote-api "https://api.github.com/repos/jeff-bruemmer/proserunner-default-checks/commits/main")

(def default-global-config-template
  "{:checks []}")

(def default-ignore-template
  "{:ignore #{}\n :ignore-issues #{}}")

(defn ^:private get-remote-zip!
  "Retrieves default checks, or times out after 5 seconds.

  Note: :timeout applies to the entire HTTP request (connect + read).

  Returns Result<byte-array> - Success with ZIP bytes, or Failure on error."
  [address]
  (result/try-result-with-context
   (fn []
     (let [resp (client/get address {:as :bytes
                                     :timeout 5000
                                     :throw false})]
       (if (= 200 (:status resp))
         (:body resp)
         (throw (ex-info "Failed to fetch remote checks"
                        {:status (:status resp)
                         :address address})))))
   {:operation :get-remote-zip :address address}))

(defn ^:private copy!
  "Copies `stream` to `out-file`, creating parent-dir if necessary.
  Used in unzip-file!"
  [stream save-path out-file]
  (file-utils/ensure-parent-dir save-path)
  (io/copy stream out-file))

(defn ^:private unzip-file!
  "Uncompress zip archive, renaming entries from old-name to new-name.

   Options map keys:
   - :input - Zip archive content (byte array or input stream source)
   - :output - Target directory path
   - :old-name - String to replace in entry names
   - :new-name - Replacement string for entry names"
  [{:keys [input output old-name new-name]}]
  (with-open [stream (-> input io/input-stream java.util.zip.ZipInputStream.)]
    (loop [entry (.getNextEntry stream)]
      (when entry
        (let [entry-name (string/replace (.getName entry) old-name new-name)
              save-path (file-utils/join-path output entry-name)
              out-file (io/file save-path)]
          (if (.isDirectory entry)
            (file-utils/mkdirs-if-missing save-path)
            (copy! stream save-path out-file))
          (recur (.getNextEntry stream)))))))

(defn ^:private get-remote-version
  "Get the latest commit SHA from GitHub API.
   Returns nil if unable to fetch (offline, rate limit, etc)."
  []
  (try
    (let [resp (client/get remote-api {:timeout 3000
                                       :throw false})]
      (when (= 200 (:status resp))
        (let [body (json/parse-string (:body resp) true)]
          (:sha body))))
    (catch Exception _ nil)))

(defn ^:private write-local-version!
  "Write the current version SHA to local version file atomically."
  [sha]
  (when sha
    (let [version-file (sys/filepath ".proserunner" ".version")]
      (file-utils/atomic-spit version-file sha))))

(defn ^:private download-checks!
  "Download and extract default checks from GitHub.

  Returns Result<nil> - Success when complete, Failure on error."
  []
  (console/status "Downloading default checks from " remote-address)
  (result/try-result-with-context
   (fn []
     (let [zip-result (get-remote-zip! remote-address)]
       (if (result/failure? zip-result)
         (throw (ex-info (:error zip-result) (:context zip-result)))
         (do
           (unzip-file! {:input (:value zip-result)
                         :output (sys/home-dir)
                         :old-name "proserunner-default-checks-main"
                         :new-name ".proserunner"})
           ;; Save the version after successful download
           (when-let [version (get-remote-version)]
             (write-local-version! version))
           nil))))
   {:operation :download-checks}))

(defn ^:private backup-directory!
  "Create a timestamped backup of a directory."
  [dir-path]
  (let [timestamp (.format (java.text.SimpleDateFormat. "yyyyMMdd-HHmmss")
                           (java.util.Date.))
        backup-dir (file-utils/join-path (sys/home-dir) (str ".proserunner-backup-" timestamp))]
    (when (.exists (io/file dir-path))
      (file-utils/mkdirs-if-missing backup-dir)
      (doseq [^java.io.File file (file-seq (io/file dir-path))]
        (when (.isFile file)
          (let [rel-path (subs (.getPath file) (count dir-path))
                target (io/file (str backup-dir rel-path))]
            (file-utils/ensure-parent-dir (.getPath target))
            (io/copy file target))))
      (console/status "Created backup at: " backup-dir)
      backup-dir)))

(defn- backup-and-preserve-files
  "Backs up existing checks and preserves config and ignore files.
  Returns map with :config-backup and :ignore-backup (may be nil)."
  [default-dir config-file ignore-file]
  (console/status "Backing up existing checks...")
  (backup-directory! default-dir)
  {:config-backup (when (.exists (io/file config-file))
                    (slurp config-file))
   :ignore-backup (when (.exists (io/file ignore-file))
                    (slurp ignore-file))})

(defn ensure-global-config!
  "Ensures ~/.proserunner/ directory and ignore.edn exist.
   Does NOT create config.edn - that should be created by --restore-defaults.
   Idempotent - safe to call multiple times."
  []
  (let [proserunner-dir (sys/filepath ".proserunner")
        ignore-path (sys/filepath ".proserunner" "ignore.edn")]
    (file-utils/mkdirs-if-missing proserunner-dir)
    (when-not (.exists (io/file ignore-path))
      (file-utils/atomic-spit ignore-path default-ignore-template))))

(defn- create-default-config-entry!
  "Creates config.edn with auto-discovered default check entry.
   Discovers .edn files in ~/.proserunner/default/ directory."
  []
  (let [config-path (sys/filepath ".proserunner" "config.edn")
        default-dir (sys/filepath ".proserunner" "default")
        edn-files (check-resolver/get-edn-files default-dir)
        config-content {:checks [{:name "default"
                                  :directory "default"
                                  :files (or edn-files [])}]}]
    (file-utils/atomic-spit config-path
      (str ";; Proserunner Global Configuration\n"
           ";; Auto-generated by --restore-defaults\n\n"
           (with-out-str (pprint/pprint config-content))))))

(defn- download-and-restore-preserved-files!
  "Downloads fresh checks and restores preserved files.
  Throws on download failure."
  [config-file ignore-file config-backup ignore-backup]
  (console/status "Downloading fresh default checks...")
  (let [dl-result (download-checks!)]
    (when (result/failure? dl-result)
      (throw (ex-info (:error dl-result) (:context dl-result)))))

  ;; Restore preserved files atomically if they existed
  (if config-backup
    (do
      (file-utils/atomic-spit config-file config-backup)
      (console/status "Preserved your config.edn"))
    (do
      (create-default-config-entry!)
      (console/status "Created config.edn with default check entry")))

  (when ignore-backup
    (file-utils/atomic-spit ignore-file ignore-backup)
    (console/status "Preserved your ignore.edn"))

  (console/status "Default checks restored.")
  (console/status "Your custom checks in ~/.proserunner/custom/ were not modified."))

(defn restore-defaults!
  "Restore default checks from GitHub, backing up existing checks first.

  Returns Result<nil> - Success when complete, Failure on error."
  []
  (console/status "Restoring default checks...")
  (result/try-result-with-context
   (fn []
     (let [proserunner-dir (sys/filepath ".proserunner")
           default-dir (sys/filepath ".proserunner" "default")
           config-file (sys/filepath ".proserunner" "config.edn")
           ignore-file (sys/filepath ".proserunner" "ignore.edn")]

       ;; Check if .proserunner directory exists
       (if-not (.exists (io/file proserunner-dir))
         (do
           (console/status "No .proserunner directory found. Creating fresh installation...")
           (let [dl-result (download-checks!)]
             (if (result/failure? dl-result)
               (throw (ex-info (:error dl-result) (:context dl-result)))
               (do
                 (console/status "Default checks installed.")
                 (create-default-config-entry!)
                 (console/status "Created config.edn with default check entry")))))
         ;; Otherwise, backup and restore
         (let [{:keys [config-backup ignore-backup]}
               (backup-and-preserve-files default-dir config-file ignore-file)]
           (download-and-restore-preserved-files! config-file ignore-file
                                                  config-backup ignore-backup)))
       nil))
   {:operation :restore-defaults}))

(defn initialize-proserunner
  "Sets up ~/.proserunner directory and downloads default checks for first-time users.

   Returns Result with config on success, or Failure with error details."
  [default-config]
  (console/status "First run: setting up ~/.proserunner")
  (let [dl-result (download-checks!)]
    (if (result/failure? dl-result)
      (result/map-err dl-result #(str "Failed to download default checks: " %))
      (do
        (console/status "Created Proserunner directory: " (sys/filepath ".proserunner/"))
        (console/status "You can store custom checks in: " (sys/filepath ".proserunner" "custom/"))
        (console/status "To update the default checks later, run: proserunner --restore-defaults")
        (create-default-config-entry!)
        (loader/load-config-from-file default-config)))))

(defn default
  "If current config isn't valid, use the default."
  [options]
  (let [cur-config (:config options)
        new-config (sys/filepath ".proserunner" "config.edn")]
    (if (or (nil? cur-config)
            (not (.exists (io/file cur-config))))
      (assoc options :config new-config)
      options)))

(defn- ensure-checks-exist!
  "Ensures that checks directories referenced by config exist.
   Downloads default checks if missing and referenced.

   Returns Result<nil> on success, or Failure with error details."
  [check-entries global-checks]
  (let [missing (check-resolver/missing-global-check-directories check-entries global-checks)]
    (if (seq missing)
      (if (contains? missing "default")
        (do
          (console/status "Default checks not found.")
          (let [default-config (sys/filepath ".proserunner" "config.edn")
                init-result (initialize-proserunner default-config)]
            (if (result/success? init-result)
              (result/ok nil)
              init-result)))
        ;; For now, only handle "default". "custom" would need similar logic
        (result/ok nil))
      (result/ok nil))))

(defn- load-project-based-config
  "Loads project configuration, merging with global config as appropriate.
   Ensures referenced check directories exist, downloading if necessary.

   Returns Result with Config record on success, or Failure with error details."
  [current-dir]
  ;; First, find and read the manifest to get raw check entries
  (if-let [{:keys [manifest-path]} (manifest/find current-dir)]
    (let [project-manifest (manifest/read manifest-path)
          global-cfg (project-config/load-global-config)
          ;; Merge configs to get final check entries list
          merged (merger/merge-configs global-cfg project-manifest)
          check-entries (:checks merged)
          global-checks (or (:checks global-cfg) [])
          ;; Ensure checks exist before proceeding
          ensure-result (ensure-checks-exist! check-entries global-checks)]
      (if (result/success? ensure-result)
        ;; Now load the full config (which will resolve checks)
        (let [project-cfg (project-config/load current-dir)]
          (result/ok (map->Config {:checks (:checks project-cfg)
                                   :ignore (:ignore project-cfg)})))
        ensure-result))
    (result/err "Failed to load project configuration. Check .proserunner/config.edn for errors."
                {:current-dir current-dir})))

(defn determine-config-strategy
  "Determines config loading strategy based on inputs.

  Returns strategy keyword: :custom | :project | :global | :initialize

  Context map keys:
  - :using-default? - Is the default config path being used?
  - :custom-exists? - Does the custom config file exist?
  - :in-project? - Is the current directory in a project?
  - :config-exists? - Does the global config file exist?

  Default checks are never updated implicitly; --restore-defaults does that."
  [{:keys [using-default? custom-exists? in-project? config-exists?]}]
  (cond
    ;; Custom config file specified via -c
    (and (not using-default?) custom-exists?)
    :custom

    ;; In project directory
    (and using-default? in-project?)
    :project

    ;; Global config exists
    config-exists?
    :global

    ;; First-time initialization
    :else
    :initialize))

(defn fetch-or-create!
  "Fetches or creates config file. Will exit on failure.
   Downloads default checks on first run only; it doesn't check for
   updates, so results don't change between runs unless the user runs
   --restore-defaults.

   If in a project directory, loads project config which may include
   project-specific checks and ignores merged with global config."
  [config-filepath]
  (when (and config-filepath (not (string? config-filepath)))
    (throw (ex-info (str "fetch-or-create! expects a string filepath, got: " (type config-filepath))
                    {:config-filepath config-filepath})))
  (let [default-config (sys/filepath ".proserunner" "config.edn")
        using-default? (or (nil? config-filepath) (= config-filepath default-config))
        checks-dir-exists? (.exists (io/file (sys/filepath ".proserunner" "default")))
        current-dir (System/getProperty "user.dir")
        in-project? (manifest/find current-dir)]

    ;; Ensure checks are initialized before doing anything else
    (when (and using-default? (not checks-dir-exists?))
      (result/result-or-exit (initialize-proserunner default-config)))

    ;; Determine strategy and execute
    (let [config-exists? (.exists (io/file default-config))
          custom-exists? (and config-filepath (.exists (io/file config-filepath)))
          strategy (determine-config-strategy
                     {:using-default? using-default?
                      :custom-exists? custom-exists?
                      :in-project? in-project?
                      :config-exists? config-exists?})]
      (case strategy
        :custom (result/result-or-exit (loader/load-config-from-file config-filepath))
        :project (result/result-or-exit (load-project-based-config current-dir))
        :global (result/result-or-exit (loader/load-config-from-file default-config))
        :initialize (result/result-or-exit (loader/load-config-from-file default-config))))))
