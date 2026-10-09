(ns proserunner.core
  "Main entry point for Proserunner CLI. Parses command-line arguments and dispatches to command handlers."
  (:gen-class)
  (:require [proserunner
             [commands :as cmd]
             [config :as conf]
             [console :as console]
             [effects :as effects]
             [error :as error]
             [fmt :as fmt]
             [output :as output]
             [result :as result]
             [text :as text]]
            [clojure.string :as str]
            [clojure.tools.cli :as cli]))

(set! *warn-on-reflection* true)

(def exit-codes
  "Exit statuses: no issues, issues found, and errors (bad input, failed runs)."
  {:ok 0 :issues 1 :error 2})

(def output-formats #{"group" "plain" "table" "verbose" "json" "edn"})

(def options
  "CLI option configuration. See:
  https://github.com/clojure/tools.cli"
  [["-b" "--code-blocks" "Include code blocks when checking. Default: skip them." :default false]
   ["-C" "--checks" "List all enabled checks with their types and descriptions."]
   ["-c" "--config CONFIG" "Use this config file instead of the global and project configs." :default nil
    :validate [text/file-exists? text/file-error-msg]]
   ["-d" "--cache-dir DIR" "Cache directory location. Priority: CLI > $PROSERUNNER_CACHE_DIR > $XDG_CACHE_HOME/proserunner > $TMPDIR/proserunner-storage"
    :default nil
    :validate [(fn [s] (and s (not (str/blank? s))))
               "Cache directory cannot be empty"]]
   ["-q" "--quoted-text" "Include quoted text when checking. Default: skip it." :default false]
   ["-e" "--exclude PATTERN" "Exclude files/dirs matching glob pattern. Can be used multiple times or comma-separated. Example: --exclude \"*.log,temp/**\""
    :default []
    :assoc-fn (fn [m k v]
                (let [patterns (if (re-find #"," v)
                                 (str/split v #",\s*")
                                 [v])]
                  (update m k (fnil into []) patterns)))]
   ["-f" "--file FILE" "File or directory to check, same as passing PATH. Directories processed recursively."
    :default nil
    :validate [text/file-exists? text/file-error-msg
               text/less-than-10-MB? text/file-size-msg]]
   ["-h" "--help" "Show this help."]
   ["-i" "--ignore IGNORE" "Deprecated: has no effect."]
   ["-n" "--no-cache" "Skip cache, force re-processing." :default false]
   ["-s" "--skip-ignore" "Skip all ignore lists for this run." :default false]
   ["-o" "--output FORMAT" "Output format: 'group' (default), 'plain', 'table', 'verbose', 'json', 'edn'."
    :default "group"
    :parse-fn str/lower-case
    :validate [output-formats "must be one of: group, plain, table, verbose, json, edn."]]
   ["-p" "--parallel-files" "Process files concurrently."
    :default false]
   ["-S" "--sequential-lines" "Process lines sequentially (for debugging)."
    :default false]
   ["-t" "--timer" "Print elapsed time to stderr." :default false]
   [nil "--quiet" "Suppress status messages and the summary line. Errors still print."]
   ["-v" "--version" "Show version."]
   ["-A" "--add-ignore SPECIMEN" "Add specimen to ignore list (applies everywhere)."]
   ["-R" "--remove-ignore SPECIMEN" "Remove specimen from ignore list."]
   ["-L" "--list-ignored" "List all ignored specimens."]
   ["-X" "--clear-ignored" "Clear all ignored specimens. Asks first when run in a terminal."]
   [nil "--force" "Don't ask for confirmation (with --clear-ignored)."]
   ["-Z" "--ignore-all" "Ignore all current findings (creates contextual ignores)."]
   ["-J" "--ignore-issues NUMBERS" "Ignore issues by number. Supports ranges: 1,3,5-7. Requires a PATH or --file."]
   ["-U" "--audit-ignores" "Find stale ignores."]
   ["-W" "--clean-ignores" "Remove stale ignores. Preview with --audit-ignores."]
   ["-D" "--restore-defaults" "Download fresh default checks from GitHub."]
   ["-a" "--add-checks SOURCE" "Import checks from directory."]
   ["-N" "--name NAME" "Custom name for imported checks (use with --add-checks)."]
   ["-G" "--global" "Apply to global config (~/.proserunner/)."]
   ["-P" "--project" "Apply to project config (.proserunner/)."]
   ["-I" "--init-project" "Create .proserunner/ with default config."]])

(def ^:private long-opts
  "Long option names, e.g. \"--file\", for did-you-mean suggestions."
  (map #(first (str/split (second %) #" ")) options))

(defn- with-paths
  "Collects --file and positional arguments into :paths."
  [{:keys [file] :as opts} arguments]
  (assoc opts :paths (vec (distinct (concat (when file [file]) arguments)))))

(defn- path-errors
  "Validates positional paths the way --file is validated."
  [arguments]
  (for [p arguments
        :let [problem (cond
                        (not (text/file-exists? p)) "no such file or directory."
                        (not (text/less-than-10-MB? p)) text/file-size-msg)]
        :when problem]
    (str p ": " problem)))

(defn- needs-global-config?
  "Help and version shouldn't create ~/.proserunner."
  [command]
  (not (#{:help :version :default} command)))

(defn- run-command
  "Validates, dispatches, and executes the command. Returns an exit code."
  [opts]
  (let [validation-result (cmd/validate-options opts)]
    (if (result/failure? validation-result)
      (do (error/message [(:error validation-result)])
          (:error exit-codes))
      (do
        (when (needs-global-config? (cmd/determine-command opts))
          (conf/ensure-global-config!))
        (let [command-result (cmd/dispatch-command opts)
              ;; Failures are printed by execute-command-result
              effect-result (effects/execute-command-result command-result)]
          (cond
            (result/failure? effect-result) (:error exit-codes)
            (pos? (or (-> effect-result :value last :issue-count) 0)) (:issues exit-codes)
            :else (:ok exit-codes)))))))

(defn- report-exception
  "Prints an exception for people. Stack traces only with PROSERUNNER_DEBUG."
  [^Throwable e expected?]
  (if expected?
    (console/error (.getMessage e))
    (do (console/error "unexpected error: " (or (.getMessage e) (.getName (class e))))
        (console/warn "Set PROSERUNNER_DEBUG=1 to see details, and please report it: "
                      console/issues-url)))
  (when (console/debug?)
    (.printStackTrace e)))

(defn reception
  "Parses command line `args` and applies the relevant function.
  Returns the expanded options, with :exit-code set."
  [args]
  (let [opts (cli/parse-opts args options :summary-fn fmt/summary)
        {:keys [options arguments errors]} opts
        expanded-options (-> (merge opts options)
                             (assoc :explicit-config? (some? (:config options)))
                             conf/default
                             (with-paths arguments))]
    (binding [console/*quiet* (boolean (:quiet options))]
      (assoc expanded-options :exit-code
             (try
               (cond
                 ;; -h works anywhere, regardless of other flags or errors
                 (:help options)
                 (do (effects/execute-command-result (cmd/dispatch-command expanded-options))
                     (:ok exit-codes))

                 (seq errors)
                 (do (error/message (error/describe errors long-opts))
                     (:error exit-codes))

                 (seq (path-errors arguments))
                 (do (error/message (path-errors arguments))
                     (:error exit-codes))

                 :else
                 (do (when (:ignore options)
                       (console/warn "--ignore is deprecated and has no effect."))
                     (run-command expanded-options)))
               (catch clojure.lang.ExceptionInfo e
                 (report-exception e true)
                 (:error exit-codes))
               (catch Exception e
                 (report-exception e false)
                 (:error exit-codes)))))))

(defn run
  "For development; same as main, except `run` returns the exit code
  instead of exiting, and doesn't shut down agents."
  [& args]
  (let [start-time (System/currentTimeMillis)
        options (assoc (reception args) :start-time start-time)]
    (output/time-elapsed options)
    (:exit-code options)))

(defn -main
  "Sends args to reception for dispatch, prints the time, shuts down agents,
  and exits with the command's exit code."
  [& args]
  (let [start-time (System/currentTimeMillis)
        options (assoc (reception args) :start-time start-time)]
    (output/time-elapsed options)
    (shutdown-agents)
    (result/*exit-fn* (:exit-code options))))
