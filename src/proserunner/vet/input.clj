(ns proserunner.vet.input
  "Input construction and file processing."
  (:gen-class)
  (:require [proserunner
             [checks :as checks]
             [config :as conf]
             [console :as console]
             [path-ignore :as path-ignore]
             [project-config :as project-conf]
             [result :as result]
             [storage :as store]
             [system :as sys]
             [text :as text]]
            [clojure.java.io :as io]
            [clojure.string :as string]))

(set! *warn-on-reflection* true)

(defrecord Input
  [^String files                ; Path to file(s) being vetted
   lines                        ; Vector of Line records to vet
   config                       ; Config record with checks and ignore settings
   checks                       ; Vector of Check records to apply
   cached-result                ; Previously cached Result for incremental computation (or nil)
   ^String output               ; Output format specification ("group", "table", "json", "edn", "verbose")
   ^Boolean no-cache            ; true to bypass cache and force re-processing
   ^Boolean parallel-lines      ; true to process lines concurrently using pmap
   project-ignore               ; Set of project-specific simple ignore patterns (strings)
   project-ignore-issues])      ; Set of contextual ignore maps with :file, :line-num, :specimen keys

(defn build-ignore-patterns
  "Merges ignore patterns from CLI flags and .proserunnerignore file for consistent filtering."
  [file exclude-patterns]
  (let [base-dir (-> file io/file .getAbsolutePath)
        ignore-patterns (path-ignore/read-proserunnerignore base-dir)]
    {:base-dir base-dir
     :patterns (concat exclude-patterns ignore-patterns)}))

(defn filter-valid-files
  "Discovers files to check, respecting ignore patterns and supported file types.
  Fails, naming the path, when there is nothing to check."
  [file {:keys [base-dir patterns]}]
  (let [files-result (->> file
                          io/file
                          file-seq
                          (map str)
                          (filter text/supported-file-type?)
                          (remove #(path-ignore/should-ignore? % base-dir patterns))
                          text/handle-invalid-file)
        types (string/join ", " text/supported-files)]
    (if (result/failure? files-result)
      (result/err (if (.isDirectory (io/file file))
                    (str file ": no files to check. Supported types: " types ".")
                    (str file ": unsupported file type. Supported types: " types "."))
                  (assoc (:context files-result) :file file))
      files-result)))

(defn process-files
  "Dispatches to parallel or sequential file processing based on configuration and file count.

  fetch-fn returns Result<lines>, this function collects all Results and combines them."
  [files fetch-fn parallel?]
  (let [num-files (count files)
        results (if (and parallel? (> num-files 1))
                  (doall (pmap fetch-fn files))
                  (map fetch-fn files))
        failures (filter result/failure? results)
        successes (filter result/success? results)]
    (if (seq failures)
      (first failures)
      (result/ok (vec (mapcat :value successes))))))

(defn get-lines-from-all-files
  "Get lines from all files, optionally processing files in parallel.
   When parallel? is true, files are processed concurrently using pmap.

   Options map keys:
   - :code-blocks - Include code blocks when checking
   - :check-quoted-text - Include quoted text when checking
   - :file - File or directory path to check
   - :exclude-patterns - Vector of glob patterns to exclude
   - :parallel? - Process files concurrently

   Returns Result<lines> - Success with all lines, or Failure on error."
  [{:keys [code-blocks check-quoted-text file exclude-patterns parallel?]}]
  (let [ignore-info (build-ignore-patterns file exclude-patterns)
        files-result (filter-valid-files file ignore-info)]
    (if (result/failure? files-result)
      files-result
      (let [files (:value files-result)
            fetch-fn #(text/fetch! code-blocks % check-quoted-text)]
        (process-files files fetch-fn parallel?)))))

(defn- determine-parallel-settings
  "Determines parallel processing settings from options.
  Files and lines aren't processed in parallel at the same time, so
  --parallel-files turns off parallel lines."
  [{:keys [parallel-files sequential-lines]}]
  {:parallel-files? (boolean parallel-files)
   :parallel-lines? (not (or sequential-lines parallel-files))})

(defn- load-config-and-dir
  "Loads configuration and check directory, preferring project config if available.
   Ensures checks are downloaded if needed before loading."
  [config project-config]
  (if (= :project (:source project-config))
    ;; For project config, still need to ensure checks exist via fetch-or-create
    ;; This triggers auto-download if default checks are missing
    {:config (conf/fetch-or-create! nil)
     :check-dir ""}
    {:config (conf/fetch-or-create! config)
     :check-dir (sys/check-dir config)}))

(defn normalize-input-options
  "Normalizes CLI options for input processing.

  Converts various option formats to standardized internal representation:
  - Exclude patterns normalized to vector
  - Parallel settings determined based on flags
  - All relevant options passed through

  Returns map with normalized keys."
  [{:keys [file exclude code-blocks quoted-text output no-cache skip-ignore config check-dir] :as options}]
  (let [exclude-patterns (cond
                           (vector? exclude) exclude
                           (string? exclude) [exclude]
                           :else [])
        {:keys [parallel-files? parallel-lines?]} (determine-parallel-settings options)]
    {:file file
     :exclude-patterns exclude-patterns
     :code-blocks code-blocks
     :quoted-text quoted-text
     :output output
     :no-cache no-cache
     :skip-ignore skip-ignore
     :parallel-files? parallel-files?
     :parallel-lines? parallel-lines?
     :sequential-lines? (:sequential-lines options)
     :config config
     :check-dir check-dir}))

(defn build-input-record
  "Builds Input record from normalized options and loaded data.

  Context map keys:
  - :normalized - Normalized options map
  - :loaded - Map with :lines and :checks keys
  - :cached - Cached result from storage (may be nil)
  - :project-ignore - Set of ignore patterns from project config
  - :project-ignore-issues - Set of ignored issue numbers

  Returns Input record."
  [{:keys [normalized loaded cached project-ignore project-ignore-issues]}]
  (map->Input
    {:file (:file normalized)
     :lines (:lines loaded)
     :config (:config normalized)
     :check-dir (:check-dir normalized)
     :checks (:checks loaded)
     :cached-result cached
     :output (:output normalized)
     :no-cache (:no-cache normalized)
     :parallel-lines (:parallel-lines? normalized)
     :project-ignore project-ignore
     :project-ignore-issues project-ignore-issues}))

(defn combine-loaded-data
  "Combines loaded lines and checks into Input record.

  Retrieves cached results from storage and assembles the Input record.
  Logs warnings if cache is corrupted.

  Arguments:
  - normalized: Normalized options map
  - lines: Vector of Line records
  - loaded-checks: Map with :checks key
  - project-ignore: Set of ignore patterns from project config
  - project-ignore-issues: Set of ignored issue numbers

  Returns Input record."
  [normalized lines loaded-checks project-ignore project-ignore-issues]
  (let [cached-result (store/get-cached-result (:file normalized) normalized)
        ;; Log corruption warnings
        _ (when (and (result/failure? cached-result)
                    (= :corrupted-cache (get-in cached-result [:context :type])))
            (console/warn "Corrupted cache for '" (:file normalized) "'; recomputing."))
        cached (result/get-value cached-result)
        loaded {:lines lines :checks (:checks loaded-checks)}]
    (build-input-record {:normalized normalized
                         :loaded loaded
                         :cached cached
                         :project-ignore project-ignore
                         :project-ignore-issues project-ignore-issues})))

(defn prepare-input-context
  "Prepares normalized config and ignore patterns from CLI options.

  Loads project configuration and determines ignore patterns based on skip-ignore flag.

  Arguments:
  - options: Raw CLI options map

  Returns context map with:
  - :normalized - Normalized options with loaded config
  - :project-config - Project configuration
  - :project-ignore - Set of ignore patterns (empty if skip-ignore)
  - :project-ignore-issues - Set of ignored issue numbers (empty if skip-ignore)"
  [options]
  (let [normalized (normalize-input-options options)
        current-dir (System/getProperty "user.dir")
        ;; An explicit --config replaces both the global and project configs
        project-config (if (:explicit-config? options)
                         (assoc (project-conf/load-global-config) :source :global)
                         (project-conf/load current-dir))
        {:keys [config check-dir]} (load-config-and-dir (:config normalized) project-config)
        project-ignore (if (:skip-ignore normalized) #{} (:ignore project-config))
        project-ignore-issues (if (:skip-ignore normalized) #{} (:ignore-issues project-config))
        normalized-with-config (assoc normalized :config config :check-dir check-dir)]
    {:normalized normalized-with-config
     :project-config project-config
     :project-ignore project-ignore
     :project-ignore-issues project-ignore-issues}))

(defn load-input-data
  "Loads lines and checks from files.

  Chains two I/O operations:
  1. Load lines from files (can fail if files don't exist or can't be read)
  2. Load checks from config (can fail if check definitions are invalid)

  Both operations return Result. If either fails, the whole operation fails.

  Arguments:
  - normalized: Normalized options map
  - project-ignore: Set of ignore patterns from project config

  Returns Result<map> with:
  - :lines - Vector of Line records
  - :loaded-checks - Map with :checks and :warnings keys"
  [normalized project-ignore]
  (let [lines-result (get-lines-from-all-files
                       {:code-blocks (:code-blocks normalized)
                        :check-quoted-text (:quoted-text normalized)
                        :file (:file normalized)
                        :exclude-patterns (:exclude-patterns normalized)
                        :parallel? (:parallel-files? normalized)})
        checks-opts (assoc normalized :project-ignore project-ignore)]
    (result/bind lines-result
      (fn [lines]
        (result/bind (checks/create checks-opts)
          (fn [loaded-checks]
            (result/ok {:lines lines :loaded-checks loaded-checks})))))))

(defn make
  "Builds Input record from CLI options.

  Returns Result<Input>."
  [options]
  (let [{:keys [normalized project-ignore project-ignore-issues]} (prepare-input-context options)]
    (result/fmap (load-input-data normalized project-ignore)
      (fn [{:keys [lines loaded-checks]}]
        (combine-loaded-data normalized lines loaded-checks
                            project-ignore project-ignore-issues)))))

(defn count-files
  "Number of files a check of `file` covers, after exclusions."
  [file options]
  (let [{:keys [exclude-patterns]} (normalize-input-options options)
        files-result (filter-valid-files file (build-ignore-patterns file exclude-patterns))]
    (if (result/success? files-result)
      (count (:value files-result))
      0)))
