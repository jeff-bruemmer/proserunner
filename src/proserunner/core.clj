(ns proserunner.core
  "Main entry point for Proserunner CLI. Parses command-line arguments and dispatches to command handlers."
  (:gen-class)
  (:require [proserunner
             [cli :as cli]
             [commands :as cmd]
             [config :as conf]
             [console :as console]
             [effects :as effects]
             [error :as error]
             [output :as output]
             [result :as result]
             [text :as text]]))

(set! *warn-on-reflection* true)

(def exit-codes
  "Exit statuses: no issues, issues found, and errors (bad input, failed runs)."
  {:ok 0 :issues 1 :error 2})

(defn- path-errors
  "Validates positional paths the way --file is validated. A missing first
  path that looks like a mistyped command gets a suggestion."
  [paths]
  (for [[i p] (map-indexed vector paths)
        :when (not (text/stdin? p))
        :let [problem (cond
                        (not (text/file-exists? p))
                        (str "no such file or directory."
                             (when-let [c (and (zero? i) (error/suggest-option p cli/commands))]
                               (str " Did you mean the '" c "' command?")))
                        (not (text/less-than-10-MB? p)) text/file-size-msg)]
        :when problem]
    (str p ": " problem)))

(defn- command-shadows-path
  "When the command word also names a file or directory here, says how to
  check that path instead."
  [command-word]
  (when (and command-word (.exists (java.io.File. ^String command-word)))
    (str "Running the '" command-word "' command. To check the path named "
         command-word ", use ./" command-word)))

(defn- needs-global-config?
  "Help and version shouldn't create the global config directory."
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
        ;; Resolve the default config only now: ensure-global-config! may
        ;; have just moved ~/.proserunner to the XDG config directory
        (let [opts (conf/default opts)
              command-result (cmd/dispatch-command opts)
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
  (let [{:keys [options summary errors warnings command-word]} (cli/parse args)
        paths (:paths options)
        expanded-options (assoc options
                                :summary summary
                                :explicit-config? (some? (:config options)))]
    (binding [console/*quiet* (boolean (:quiet options))]
      (assoc expanded-options :exit-code
             (try
               (cond
                 ;; -h works anywhere, regardless of other flags or errors
                 (:help options)
                 (do (effects/execute-command-result (cmd/dispatch-command (conf/default expanded-options)))
                     (:ok exit-codes))

                 (seq errors)
                 (do (error/message (error/describe errors cli/long-opts))
                     (:error exit-codes))

                 (seq (path-errors paths))
                 (do (error/message (path-errors paths))
                     (:error exit-codes))

                 :else
                 (do (doseq [w warnings] (console/warn w))
                     (some-> (command-shadows-path command-word) console/status)
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
