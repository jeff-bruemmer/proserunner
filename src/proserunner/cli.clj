(ns proserunner.cli
  "Command-line parsing: options, commands, and the old flags that still
  work with a warning.

  Commands are a front end for the action options commands.clj dispatches
  on: `proserunner ignore add hopefully` sets :add-ignore, just as the
  deprecated `--add-ignore hopefully` does."
  (:require [clojure.string :as str]
            [clojure.tools.cli :as cli]
            [proserunner.fmt :as fmt]
            [proserunner.text :as text]))

(set! *warn-on-reflection* true)

(def output-formats #{"group" "plain" "table" "verbose" "json" "edn"})

(def options
  "Options shown in help. Single letters are kept for the common ones."
  [["-c" "--config CONFIG" "Use this config file instead of the global and project configs." :default nil
    :validate [text/file-exists? text/file-error-msg]]
   ["-e" "--exclude PATTERN" "Exclude files/dirs matching glob pattern. Can be used multiple times or comma-separated. Example: --exclude \"*.log,temp/**\""
    :default []
    :assoc-fn (fn [m k v]
                (let [patterns (if (re-find #"," v)
                                 (str/split v #",\s*")
                                 [v])]
                  (update m k (fnil into []) patterns)))]
   ["-f" "--file FILE" "File or directory to check, same as passing PATH."
    :default nil
    :validate [#(or (text/stdin? %) (text/file-exists? %)) text/file-error-msg
               text/less-than-10-MB? text/file-size-msg]]
   ["-h" "--help" "Show help. After a command, show that command's help."]
   ["-o" "--output FORMAT" "Output format: 'group' (default), 'plain', 'table', 'verbose', 'json', 'edn'."
    :default "group"
    :parse-fn str/lower-case
    :validate [output-formats "must be one of: group, plain, table, verbose, json, edn."]]
   ["-q" "--quiet" "Suppress status messages and the summary line. Errors still print."]
   ["-v" "--version" "Show version."]
   [nil "--code-blocks" "Include code blocks when checking. Default: skip them." :default false]
   [nil "--quoted-text" "Include quoted text when checking. Default: skip it." :default false]
   [nil "--skip-ignore" "Skip all ignore lists for this run." :default false]
   [nil "--ignore-issues NUMBERS" "Ignore issues from this run by number. Supports ranges: 1,3,5-7."]
   [nil "--ignore-all" "Ignore every issue from this run."]
   [nil "--global" "Use the global config, even inside a project."]
   [nil "--project" "Use the project config (.proserunner/)."]
   [nil "--force" "With ignore clear: don't ask for confirmation."]
   [nil "--name NAME" "With checks add: name for the imported checks. Default: the directory's name."]
   [nil "--cache-dir DIR" "Cache directory location. Priority: CLI > $PROSERUNNER_CACHE_DIR > $XDG_CACHE_HOME/proserunner > $TMPDIR/proserunner-storage"
    :default nil
    :validate [(fn [s] (and s (not (str/blank? s))))
               "Cache directory cannot be empty"]]
   [nil "--no-cache" "Skip cache, force re-processing." :default false]
   [nil "--parallel-files" "Process files concurrently." :default false]
   [nil "--sequential-lines" "Process lines sequentially (for debugging)." :default false]
   [nil "--timer" "Print elapsed time to stderr." :default false]])

;;;; Deprecated flags

(def deprecated-flags
  "Flags replaced by commands, with what to use instead. They still work,
  with a warning. A nil replacement means the flag has no effect."
  {"--add-ignore" "proserunner ignore add SPECIMEN"
   "--remove-ignore" "proserunner ignore remove SPECIMEN"
   "--list-ignored" "proserunner ignore list"
   "--clear-ignored" "proserunner ignore clear"
   "--audit-ignores" "proserunner ignore audit"
   "--clean-ignores" "proserunner ignore clean"
   "--checks" "proserunner checks"
   "--add-checks" "proserunner checks add DIR"
   "--restore-defaults" "proserunner checks restore"
   "--init-project" "proserunner init"
   "--ignore" nil})

(def ^:private deprecated-options
  "Specs for deprecated-flags, hidden from help."
  [[nil "--add-ignore SPECIMEN"]
   [nil "--remove-ignore SPECIMEN"]
   [nil "--list-ignored"]
   [nil "--clear-ignored"]
   [nil "--audit-ignores"]
   [nil "--clean-ignores"]
   [nil "--checks"]
   [nil "--add-checks SOURCE"]
   [nil "--restore-defaults"]
   [nil "--init-project"]
   [nil "--ignore IGNORE"]])

(def deprecated-shorts
  "Single-letter flags retired so letters stay free for common options.
  Each still works as its long form, with a warning."
  {"-A" "--add-ignore" "-R" "--remove-ignore" "-L" "--list-ignored"
   "-X" "--clear-ignored" "-U" "--audit-ignores" "-W" "--clean-ignores"
   "-C" "--checks" "-a" "--add-checks" "-D" "--restore-defaults"
   "-I" "--init-project" "-i" "--ignore"
   "-Z" "--ignore-all" "-J" "--ignore-issues" "-N" "--name"
   "-G" "--global" "-P" "--project"
   "-b" "--code-blocks" "-s" "--skip-ignore" "-d" "--cache-dir" "-n" "--no-cache"
   "-p" "--parallel-files" "-S" "--sequential-lines" "-t" "--timer"})

(defn- long-name
  "\"--add-ignore SPECIMEN\" -> \"--add-ignore\""
  [long-opt]
  (first (str/split long-opt #" ")))

(defn- long-id
  "\"--add-ignore\" -> :add-ignore"
  [long-opt]
  (keyword (subs long-opt 2)))

(defn- short-id
  "\"-A\" -> :deprecated-short/A"
  [short-opt]
  (keyword "deprecated-short" (subs short-opt 1)))

(def ^:private deprecated-short-options
  "A hidden spec for each deprecated short flag: the long option's spec
  under its own id, without the default, so tools.cli only sets it when
  the short flag is used. Clusters like -bt keep working."
  (let [by-name (into {} (map (juxt #(long-name (second %)) identity))
                      (concat options deprecated-options))]
    (for [[short-opt long-opt] (sort deprecated-shorts)
          :let [[_ spec-long _ & kvs] (by-name long-opt)
                arg (second (str/split spec-long #" "))
                kvs (->> (partition 2 kvs)
                         (remove (comp #{:default :default-fn} first))
                         (apply concat))]]
      (into [short-opt nil "" :id (short-id short-opt)]
            (concat (when arg [:required arg]) kvs)))))

(def all-options
  "Every spec tools.cli parses with."
  (vec (concat options deprecated-options deprecated-short-options)))

(def long-opts
  "Long option names, e.g. \"--file\", for did-you-mean suggestions."
  (map #(long-name (second %)) options))

(defn- deprecation-message
  [flag replacement]
  (if replacement
    (str flag " is deprecated; use '" replacement "' instead.")
    (str flag " is deprecated and has no effect.")))

(defn- expand-deprecated
  "Moves values set by deprecated short flags to their long option, and
  returns {:options :warnings} with one warning per deprecated flag used."
  [options]
  (let [shorts (filter #(contains? options (short-id (key %))) (sort deprecated-shorts))
        from-short (set (map (comp long-id val) shorts))
        options (reduce (fn [m [short-opt long-opt]]
                          (-> m
                              (dissoc (short-id short-opt))
                              (assoc (long-id long-opt) (get m (short-id short-opt)))))
                        options
                        shorts)
        short-warnings (for [[short-opt long-opt] shorts]
                         (deprecation-message short-opt (if (contains? deprecated-flags long-opt)
                                                          (get deprecated-flags long-opt)
                                                          long-opt)))
        long-warnings (for [[flag replacement] (sort deprecated-flags)
                            :let [id (long-id flag)]
                            :when (and (contains? options id) (not (from-short id)))]
                        (deprecation-message flag replacement))]
    {:options options
     :warnings (vec (concat short-warnings long-warnings))}))

;;;; Commands

(def commands
  "Command names. To check a path with one of these names, write ./name."
  #{"check" "ignore" "checks" "init" "help"})

(def subcommands
  "Second words for ignore and checks, with the option each sets and the
  argument it takes, if any."
  {"ignore" (array-map "add" {:id :add-ignore :arg "SPECIMEN"}
                       "remove" {:id :remove-ignore :arg "SPECIMEN"}
                       "rm" {:id :remove-ignore :arg "SPECIMEN"}
                       "list" {:id :list-ignored}
                       "ls" {:id :list-ignored}
                       "clear" {:id :clear-ignored}
                       "audit" {:id :audit-ignores}
                       "clean" {:id :clean-ignores})
   "checks" (array-map "list" {:id :checks}
                       "add" {:id :add-checks :arg "DIR"}
                       "restore" {:id :restore-defaults})})

(defn- too-many
  "Error for a command given more arguments than it takes."
  [cmd arg args]
  (if arg
    (str "'" cmd "' takes one " arg ", but got " (count args) "."
         (when (= arg "SPECIMEN")
           (str " To ignore a phrase, quote it: proserunner " cmd " \""
                (str/join " " args) "\"")))
    (str "'" cmd "' doesn't take arguments, but got: " (str/join " " args))))

(defn- two-word-command
  "Parses `group sub args...`, e.g. ignore add hopefully."
  [group [sub & args]]
  (let [table (get subcommands group)
        {:keys [id arg]} (get table sub)
        cmd (str group " " sub)]
    (cond
      ;; `checks` alone lists checks; `ignore` alone shows its help
      (and (nil? sub) (= group "checks")) {:options {:checks true}}
      (nil? sub) {:options {:help true}}
      (nil? id) {:error (str "unknown " group " command '" sub "'. Commands: "
                             (str/join ", " (keys table)) ".")}
      (and arg (empty? args)) {:error (str "'" cmd "' needs a " arg
                                           ". Example: proserunner " cmd " "
                                           (if (= arg "DIR") "~/my-checks" "hopefully"))}
      (> (count args) (if arg 1 0)) {:error (too-many cmd arg args)}
      :else {:options {id (if arg (first args) true)}})))

(defn parse-command
  "Interprets positional arguments. Returns a map with :options to merge,
  :paths, and :help-topic (the command named, for -h), or :error."
  [[word & more :as arguments]]
  (let [topic (when (commands word) (if (= word "help") (first more) word))]
    (assoc
     (case word
       "check" {:paths (vec more)}
       ("ignore" "checks") (two-word-command word more)
       "init" (if (seq more)
                {:error (too-many "init" nil more)}
                {:options {:init-project true}})
       "help" (cond
                (and (first more) (not (commands (first more))))
                {:error (str "no help for '" (first more) "'. Commands: "
                             (str/join ", " (sort commands)) ".")}
                (next more) {:error (too-many "help" "COMMAND" more)}
                :else {:options {:help true}})
       {:paths (vec arguments)})
     :help-topic topic)))

;;;; Parsing

(defn- shown-in-help?
  [{:keys [long-opt]}]
  (and long-opt (not (contains? deprecated-flags long-opt))))

(defn parse
  "Parses `args`. Returns a map with:
  - :options   - option and command settings, :paths, and :help-topic
  - :summary   - the help listing of options
  - :errors    - tools.cli errors and command errors
  - :warnings  - one per deprecated flag used
  - :command-word - the command named, if any, e.g. \"checks\""
  [args]
  (let [{:keys [options arguments errors summary]}
        (cli/parse-opts args all-options
                        :summary-fn #(fmt/summary (filter shown-in-help? %)))
        {:keys [options warnings]} (expand-deprecated options)
        {:keys [paths help-topic error] :as command} (parse-command arguments)]
    {:options (-> (merge options (:options command))
                  (assoc :paths (vec (distinct (concat (when (:file options) [(:file options)])
                                                       paths))))
                  (cond-> help-topic (assoc :help-topic help-topic)))
     :arguments (vec paths)
     :summary summary
     :errors (cond-> (vec errors) error (conj error))
     :warnings warnings
     :command-word (when (commands (first arguments)) (first arguments))}))
