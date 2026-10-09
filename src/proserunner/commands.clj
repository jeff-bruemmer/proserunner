(ns proserunner.commands
  "Command handlers that return effect descriptions.

  Handlers determine what operations to perform based on CLI options,
  returning data structures that describe effects to execute. This separates
  command logic from I/O operations.

  Each handler returns a map with:
  - :effects   - vector of effect tuples [effect-type & args]
  - :messages  - optional vector of user messages
  - :format-fn - optional function to format effect results

  The effects namespace executes these descriptions."
  (:gen-class)
  (:require [proserunner.context :as context]
            [proserunner.config.manifest :as manifest]
            [proserunner.ignore.audit :as ignore-audit]
            [proserunner.output.format :as output-fmt]
            [proserunner.result :as result]
            [clojure.string :as str]))

(defn project-exists?
  "Checks if a .proserunner/config.edn file exists in the current directory."
  []
  (let [start-dir (System/getProperty "user.dir")]
    (boolean (manifest/find start-dir))))

(defn determine-target
  "Determines whether to use :project or :global based on flags and project existence.

  Logic:
  - If --global flag is explicitly set, use :global
  - If --project flag is explicitly set, use :project
  - If .proserunner/config.edn exists, default to :project
  - Otherwise, default to :global

  Throws ExceptionInfo if both --global and --project are specified, or if
  --project is specified but no project exists."
  [opts]
  (:target (context/determine-context opts)))

(defn get-target-context
  "Returns a map with target scope information for commands.

  Options:
  - :use-determine-target? - if true, validates project existence; otherwise uses flags directly
  - :include-alt-msg - if true, includes :alt-msg suggesting alternate scope
  - :alt-msg-template - custom alternate message template

  Returns:
  - :target - either :project or :global keyword
  - :msg-context - string for display (\"project\" or \"global\")
  - :alt-msg - optional string suggesting the alternate scope
  - :opts-with-target - opts map with :project key set correctly"
  ([opts]
   (get-target-context opts nil))
  ([opts {:keys [include-alt-msg alt-msg-template use-determine-target?]}]
   (if use-determine-target?
     ;; Full context resolution with project existence validation
     (let [context-opts (cond-> opts
                          include-alt-msg (assoc :include-alt-msg true)
                          alt-msg-template (assoc :alt-msg-template alt-msg-template)
                          true (assoc :normalize-opts true))
           result (context/determine-context context-opts)]
       (cond-> {:target (:target result)
                :msg-context (:msg-context result)
                :opts-with-target (:opts-with-target result)}
         (:alt-msg result) (assoc :alt-msg (:alt-msg result))))

     ;; Simple mode: use flags directly without validation
     (let [target (if (:project opts) :project :global)
           msg-context (if (= target :project) "project" "global")
           base-result {:target target
                        :msg-context msg-context
                        :opts-with-target (assoc opts :project (= target :project))}]
       (if include-alt-msg
         (assoc base-result :alt-msg
                (or alt-msg-template
                    (if (= target :project)
                      "Use --global to add to global ignore list instead."
                      "Use --project to add to project ignore list instead.")))
         base-result)))))

(set! *warn-on-reflection* true)

;;;; Issue number parsing utilities

(def ^:private positive-number-error
  "Issue numbers must be positive")

(defn- parse-single-number
  "Parses a single issue number string. Returns the number if valid (positive integer).
  Throws ex-info if invalid."
  [s]
  (let [num (Integer/parseInt (str/trim s))]
    (when (< num 1)
      (throw (ex-info positive-number-error {:number num})))
    num))

(defn- parse-range
  "Parses an issue number range like '1-5'. Returns sequence of numbers.
  Throws ex-info if malformed or invalid."
  [s]
  (let [range-parts (str/split s #"-")]
    (when (not= 2 (count range-parts))
      (throw (ex-info "Malformed range - use format 'N-M'" {:part s})))
    (let [[start end] range-parts
          start-num (Integer/parseInt (str/trim start))
          end-num (Integer/parseInt (str/trim end))]
      (when (< start-num 1)
        (throw (ex-info positive-number-error {:number start-num})))
      (when (< end-num 1)
        (throw (ex-info positive-number-error {:number end-num})))
      (when (> start-num end-num)
        (throw (ex-info (str "Invalid range '" s "' - start must be less than or equal to end")
                       {:start start-num :end end-num})))
      (range start-num (inc end-num)))))

(defn- parse-part
  "Parses a single part from comma-separated input. Returns sequence of numbers.
  Handles both single numbers ('3') and ranges ('1-5')."
  [part]
  (let [trimmed (str/trim part)]
    (when-not (str/blank? trimmed)
      (if (str/includes? trimmed "-")
        (parse-range trimmed)
        [(parse-single-number trimmed)]))))

(defn parse-issue-numbers
  "Parses a string of issue numbers and ranges into a sorted vector of unique numbers.
  Returns Result containing vector of numbers, or error if parsing fails.

  Examples:
    \"1\" -> (result/ok [1])
    \"1,3,5\" -> (result/ok [1 3 5])
    \"1-5\" -> (result/ok [1 2 3 4 5])
    \"1-3,5,7-9\" -> (result/ok [1 2 3 5 7 8 9])
    \"foo\" -> (result/err \"Invalid issue number format: ...\")
    \"\" -> (result/err \"Issue numbers cannot be empty\")"
  [s]
  (if (or (nil? s) (str/blank? s))
    (result/err "Issue numbers cannot be empty"
                {:input s :operation :parse-issue-numbers})
    (try
      (let [parts (str/split s #",")
            numbers (->> parts
                        (mapcat parse-part)
                        (remove nil?)
                        (distinct)
                        (sort)
                        (vec))]
        (result/ok numbers))
      (catch NumberFormatException e
        (result/err (str "Invalid issue number format: " (.getMessage e))
                    {:input s :operation :parse-issue-numbers :exception-type :number-format}))
      (catch clojure.lang.ExceptionInfo e
        (result/err (.getMessage e)
                    {:input s :operation :parse-issue-numbers :exception-type :validation}))
      (catch Exception e
        (result/err (str "Unexpected error parsing issue numbers: " (.getMessage e))
                    {:input s :operation :parse-issue-numbers :exception-type :unknown})))))


(defn filter-issues-by-numbers
  "Filters a sequence of issues to only those at the specified 1-based indices.
  
  Example:
    (filter-issues-by-numbers [issue1 issue2 issue3] [1 3])
    => [issue1 issue3]"
  [issues numbers]
  (let [number-set (set numbers)]
    (->> issues
         (map-indexed (fn [idx issue] [(inc idx) issue]))
         (filter (fn [[num _]] (contains? number-set num)))
         (map second)
         (vec))))

(defn handle-add-ignore
  "Handler for adding a specimen to ignore list."
  [{:keys [add-ignore] :as opts}]
  (let [{:keys [msg-context alt-msg]} (get-target-context opts {:include-alt-msg true
                                                                  :use-determine-target? true})]
    {:effects [[:ignore/add add-ignore opts]]
     :messages [(format "Added to %s ignore list: %s" msg-context add-ignore)
                alt-msg]}))

(defn handle-remove-ignore
  "Handler for removing a specimen from ignore list.
  Resolves the scope the same way the effect does, so the message names the right list."
  [{:keys [remove-ignore] :as opts}]
  (let [{:keys [msg-context opts-with-target]} (get-target-context opts {:use-determine-target? true})]
    {:effects [[:ignore/remove remove-ignore opts-with-target]]
     :messages [(format "Removed from %s ignore list: %s" msg-context remove-ignore)]}))

(defn handle-list-ignored
  "Handler for listing all ignored specimens."
  [opts]
  {:effects [[:ignore/list opts]]
   :format-fn output-fmt/ignored-list})

(defn handle-clear-ignored
  "Handler for clearing all ignored specimens.
  The effect confirms interactively and reports what it cleared."
  [opts]
  (let [{:keys [opts-with-target]} (get-target-context opts {:use-determine-target? true})]
    {:effects [[:ignore/clear opts-with-target]]}))

(defn handle-ignore-all
  "Handler for ignoring all current findings.
   Runs the file through proserunner, collects all issues, and adds them as contextual ignores.
   Defaults to project if .proserunner/config.edn exists, otherwise global."
  [opts]
  (let [{:keys [msg-context opts-with-target]} (get-target-context opts {:use-determine-target? true})]
    {:effects [[:ignore/add-all opts-with-target]]
     :progress [(format "Adding all current findings to %s ignore list..." msg-context)]}))

(defn handle-ignore-issues
  "Handler for ignoring specific issues by number.
  Re-runs proserunner to get the same issue numbering, then creates
  contextual ignores for only the specified issue numbers.
  Defaults to project if .proserunner/config.edn exists, otherwise global."
  [{:keys [ignore-issues] :as opts}]
  (let [{:keys [msg-context opts-with-target]} (get-target-context opts {:use-determine-target? true})
        parse-result (parse-issue-numbers ignore-issues)]
    (if (result/success? parse-result)
      (let [issue-nums (result/get-value parse-result)]
        {:effects [[:ignore/add-issues issue-nums opts-with-target]]
         :progress [(format "Ignoring issues %s in %s ignore list..."
                            (str/join ", " issue-nums)
                            msg-context)]})
      {:error (:error parse-result)})))

(defn handle-audit-ignores
  "Handler for auditing ignore entries to find stale ones."
  [opts]
  {:effects [[:ignore/audit opts]]
   :format-fn ignore-audit/format-report})

(defn handle-clean-ignores
  "Handler for cleaning stale ignore entries."
  [opts]
  (let [{:keys [msg-context opts-with-target]} (get-target-context opts {:use-determine-target? true})]
    {:effects [[:ignore/clean opts-with-target]]
     :progress [(format "Cleaning stale ignores from %s ignore list..." msg-context)]}))

(defn handle-restore-defaults
  "Handler for restoring default checks from GitHub."
  [_opts]
  {:effects [[:config/restore-defaults]]})

(defn handle-init-project
  "Handler for initializing project configuration."
  [_opts]
  {:effects [[:project/init]]
   :format-fn output-fmt/init-project})

(defn handle-add-checks
  "Handler for adding custom checks from a directory."
  [{:keys [add-checks] :as opts}]
  {:effects [[:checks/add add-checks (select-keys opts [:name :global :project])]]})

(defn handle-file
  "Handler for processing a file or directory."
  [opts]
  {:effects [[:file/process opts]]})

(defn handle-checks
  "Handler for listing enabled checks."
  [{:keys [config]}]
  {:effects [[:checks/print config]]})

(defn handle-help
  "Handler for printing help/usage."
  [opts]
  {:effects [[:help/print opts]]})

(defn handle-version
  "Handler for printing version."
  [_opts]
  {:effects [[:version/print]]})

(defn handle-default
  "Handler for running with no action: print concise usage."
  [opts]
  {:effects [[:help/print-concise opts]]})

;; Command handler registry
(def handlers
  "Map of command keywords to their handler functions."
  {:add-ignore       handle-add-ignore
   :remove-ignore    handle-remove-ignore
   :list-ignored     handle-list-ignored
   :clear-ignored    handle-clear-ignored
   :ignore-all       handle-ignore-all
   :ignore-issues    handle-ignore-issues
   :audit-ignores    handle-audit-ignores
   :clean-ignores    handle-clean-ignores
   :restore-defaults handle-restore-defaults
   :init-project     handle-init-project
   :add-checks       handle-add-checks
   :file             handle-file
   :checks           handle-checks
   :help             handle-help
   :version          handle-version
   :default          handle-default})

;; Command determination
(defn determine-command
  "Determines which command to execute based on options.
  Returns a keyword identifying the command. Help and version win over
  everything else, so -h can be added to any command line."
  [{:keys [add-ignore remove-ignore list-ignored clear-ignored ignore-all ignore-issues
           audit-ignores clean-ignores
           restore-defaults init-project add-checks
           file paths checks help version]}]
  (cond
    help             :help
    version          :version
    add-ignore       :add-ignore
    remove-ignore    :remove-ignore
    list-ignored     :list-ignored
    clear-ignored    :clear-ignored
    ignore-all       :ignore-all
    ignore-issues    :ignore-issues
    audit-ignores    :audit-ignores
    clean-ignores    :clean-ignores
    restore-defaults :restore-defaults
    init-project     :init-project
    add-checks       :add-checks
    (or file (seq paths)) :file
    checks           :checks
    :else            :default))

(def ^:private actions
  "Action flags in the order determine-command checks them."
  [[:add-ignore "--add-ignore"] [:remove-ignore "--remove-ignore"]
   [:list-ignored "--list-ignored"] [:clear-ignored "--clear-ignored"]
   [:ignore-all "--ignore-all"] [:ignore-issues "--ignore-issues"]
   [:audit-ignores "--audit-ignores"] [:clean-ignores "--clean-ignores"]
   [:restore-defaults "--restore-defaults"] [:init-project "--init-project"]
   [:add-checks "--add-checks"] [:file "PATH"] [:checks "--checks"]])

(defn conflicting-actions-warning
  "Returns a warning when more than one action is requested, naming the one
  that runs and the ones that are ignored. Returns nil otherwise.
  --ignore-all and --ignore-issues need a PATH, so that pairing doesn't count."
  [opts]
  (let [present? (fn [[k _]]
                   (if (= k :file)
                     (boolean (or (:file opts) (seq (:paths opts))))
                     (boolean (get opts k))))
        requested (filter present? actions)
        requested (if (some #{:ignore-all :ignore-issues} (map first requested))
                    (remove #(= :file (first %)) requested)
                    requested)]
    (when (> (count requested) 1)
      (format "Only one action runs at a time: running %s, ignoring %s."
              (second (first requested))
              (str/join ", " (map second (rest requested)))))))

(defn dispatch-command
  "Dispatches command based on options, returning effect description.

  Returns a map with :command, :effects, and optionally :progress, :messages,
  :format-fn, or :warnings (when more than one action was requested)."
  [opts]
  (let [cmd (determine-command opts)
        handler (get handlers cmd handle-default)
        warning (when-not (#{:help :version} cmd)
                  (conflicting-actions-warning opts))]
    (cond-> (assoc (handler opts) :command cmd)
      warning (assoc :warnings [warning]))))

;; Validation
(defn validate-options
  "Validates options for conflicts and errors.
  Returns Success with options or Failure with error messages."
  [{:keys [global project ignore-issues ignore-all file paths] :as opts}]
  (cond
    (and global project)
    (result/err "Cannot specify both --global and --project flags."
                {:operation :validate-options
                 :conflict :config-scope
                 :flags [:global :project]})

    (and ignore-issues (not (or file (seq paths))))
    (result/err "--ignore-issues needs the PATH you checked, so the issue numbers match. Example: proserunner doc.md --ignore-issues 1,3"
                {:operation :validate-options
                 :missing-flag :file
                 :dependent-flag :ignore-issues})

    (and ignore-all (not (or file (seq paths))))
    (result/err "--ignore-all needs the PATH whose findings to ignore. Example: proserunner doc.md --ignore-all"
                {:operation :validate-options
                 :missing-flag :file
                 :dependent-flag :ignore-all})

    :else
    (result/ok opts)))
