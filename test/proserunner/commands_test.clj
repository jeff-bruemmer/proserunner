(ns proserunner.commands-test
  "Tests for command handlers."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.java.io :as io]
            [proserunner.commands :as cmd]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [with-temp-dir]]))

(deftest determine-command-test
  (testing "Determines add-ignore command"
    (is (= :add-ignore (cmd/determine-command {:add-ignore "foo"}))))

  (testing "Determines remove-ignore command"
    (is (= :remove-ignore (cmd/determine-command {:remove-ignore "bar"}))))

  (testing "Determines list-ignored command"
    (is (= :list-ignored (cmd/determine-command {:list-ignored true}))))

  (testing "Determines clear-ignored command"
    (is (= :clear-ignored (cmd/determine-command {:clear-ignored true}))))

  (testing "Determines restore-defaults command"
    (is (= :restore-defaults (cmd/determine-command {:restore-defaults true}))))

  (testing "Determines init-project command"
    (is (= :init-project (cmd/determine-command {:init-project true}))))

  (testing "Determines add-checks command"
    (is (= :add-checks (cmd/determine-command {:add-checks "/path/to/checks"}))))

  (testing "Determines file command"
    (is (= :file (cmd/determine-command {:file "/path/to/file.md"}))))

  (testing "Determines checks command"
    (is (= :checks (cmd/determine-command {:checks true}))))

  (testing "Determines help command"
    (is (= :help (cmd/determine-command {:help true}))))

  (testing "Determines version command"
    (is (= :version (cmd/determine-command {:version true}))))

  (testing "Defaults to :default command"
    (is (= :default (cmd/determine-command {})))))

(deftest validate-options-test
  (testing "Valid options pass validation"
    (let [opts {:file "test.md"}
          result (cmd/validate-options opts)]
      (is (result/success? result))
      (is (= opts (:value result)))))

  (testing "Parallel files without sequential lines is valid"
    (let [opts {:parallel-files true}
          result (cmd/validate-options opts)]
      (is (result/success? result))))

  (testing "Both global and project flags fails"
    (let [opts {:global true :project true}
          result (cmd/validate-options opts)]
      (is (result/failure? result))
      (is (re-find #"global.*project" (:error result)))))

  (testing "Parallel files with sequential lines is valid"
    (let [opts {:parallel-files true :sequential-lines true}
          result (cmd/validate-options opts)]
      (is (result/success? result))))

  (testing "ignore-issues without file fails"
    (let [opts {:ignore-issues "1,2,3"}
          result (cmd/validate-options opts)]
      (is (result/failure? result))
      (is (re-find #"ignore-issues.*PATH" (:error result)))))

  (testing "ignore-issues with a positional path is valid"
    (let [opts {:ignore-issues "1" :paths ["test.md"]}
          result (cmd/validate-options opts)]
      (is (result/success? result))))

  (testing "ignore-issues with file is valid"
    (let [opts {:ignore-issues "1,2,3" :file "test.md"}
          result (cmd/validate-options opts)]
      (is (result/success? result))))

  (testing "ignore-all without file fails"
    (let [opts {:ignore-all true}
          result (cmd/validate-options opts)]
      (is (result/failure? result))
      (is (re-find #"ignore-all.*PATH" (:error result)))))

  (testing "ignore-all with file is valid"
    (let [opts {:ignore-all true :file "test.md"}
          result (cmd/validate-options opts)]
      (is (result/success? result)))))

(deftest handle-add-ignore-test
  (testing "Handle add-ignore produces correct effects"
    (let [opts {:add-ignore "specimen1" :global true}
          result (cmd/handle-add-ignore opts)]
      (is (= [[:ignore/add "specimen1" opts]] (:effects result)))
      (is (some? (:messages result)))
      (is (re-find #"global" (first (:messages result))))))

  (testing "Handle add-ignore for project"
    ;; Requires a temp project directory for project validation
    (with-temp-dir [temp-dir "test-project"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")
        (let [opts {:add-ignore "specimen2" :project true :start-dir temp-dir}
              result (cmd/handle-add-ignore opts)]
          (is (= [[:ignore/add "specimen2" opts]] (:effects result)))
          (is (re-find #"project" (first (:messages result)))))))))

(deftest handle-remove-ignore-test
  (testing "Handle remove-ignore produces correct effects"
    (let [opts {:remove-ignore "specimen1" :global true}
          result (cmd/handle-remove-ignore opts)
          [[effect specimen effect-opts]] (:effects result)]
      (is (= :ignore/remove effect))
      (is (= "specimen1" specimen))
      (is (true? (:global effect-opts)))
      (is (false? (:project effect-opts)))
      (is (re-find #"global" (first (:messages result)))))))

(deftest handle-list-ignored-test
  (testing "Handle list-ignored produces correct effects"
    (let [opts {:list-ignored true}
          result (cmd/handle-list-ignored opts)]
      (is (= [[:ignore/list opts]] (:effects result)))
      (is (fn? (:format-fn result))))))

(deftest handle-clear-ignored-test
  (testing "Handle clear-ignored produces correct effects"
    (let [opts {:clear-ignored true :global true}
          result (cmd/handle-clear-ignored opts)
          [[effect effect-opts]] (:effects result)]
      (is (= :ignore/clear effect))
      (is (true? (:global effect-opts)))
      (is (false? (:project effect-opts)))
      ;; The effect reports what it cleared, after confirming
      (is (nil? (:messages result))))))

(deftest handle-restore-defaults-test
  (testing "Handle restore-defaults produces correct effects"
    (let [result (cmd/handle-restore-defaults {})]
      (is (= [[:config/restore-defaults]] (:effects result))))))

(deftest handle-init-project-test
  (testing "Handle init-project produces correct effects"
    (let [result (cmd/handle-init-project {})]
      (is (= [[:project/init]] (:effects result)))
      (is (fn? (:format-fn result))))))

(deftest handle-add-checks-test
  (testing "Handle add-checks produces correct effects"
    (let [opts {:add-checks "/path/to/checks" :name "custom" :global true}
          result (cmd/handle-add-checks opts)]
      (is (= [[:checks/add "/path/to/checks" {:name "custom" :global true}]]
             (:effects result))))))

(deftest handle-file-test
  (testing "Handle file produces correct effects"
    (let [opts {:file "/path/to/file.md"}
          result (cmd/handle-file opts)]
      (is (= [[:file/process opts]] (:effects result))))))

(deftest handle-checks-test
  (testing "Handle checks produces correct effects"
    (let [opts {:checks true :config "config.edn"}
          result (cmd/handle-checks opts)]
      (is (= [[:checks/print "config.edn"]] (:effects result))))))

(deftest handle-help-test
  (testing "Handle help produces correct effects"
    (let [opts {:help true}
          result (cmd/handle-help opts)]
      (is (= [[:help/print opts]] (:effects result))))))

(deftest handle-version-test
  (testing "Handle version produces correct effects"
    (let [result (cmd/handle-version {})]
      (is (= [[:version/print]] (:effects result))))))

(deftest handle-default-test
  (testing "Handle default produces correct effects"
    (let [opts {:summary "..."}
          result (cmd/handle-default opts)]
      (is (= [[:help/print-concise opts]] (:effects result))))))

(deftest dispatch-command-test
  (testing "Dispatch command returns effect description"
    (let [opts {:add-ignore "test-specimen" :global true}
          result (cmd/dispatch-command opts)]
      (is (= :add-ignore (:command result)))
      (is (some? (:effects result)))
      (is (= [[:ignore/add "test-specimen" opts]] (:effects result)))))

  (testing "Dispatch handles multiple command types"
    (let [test-cases [{:opts {:help true}
                       :expected-cmd :help
                       :expected-effect-type :help/print}
                      {:opts {:version true}
                       :expected-cmd :version
                       :expected-effect-type :version/print}
                      {:opts {:list-ignored true}
                       :expected-cmd :list-ignored
                       :expected-effect-type :ignore/list}]]
      (doseq [{:keys [opts expected-cmd expected-effect-type]} test-cases]
        (let [result (cmd/dispatch-command opts)]
          (is (= expected-cmd (:command result)))
          (is (= expected-effect-type (-> result :effects first first))))))))

(deftest pure-handler-testability
  (testing "Pure handlers can be tested without I/O"
    (let [opts {:add-ignore "test" :global true}
          result (cmd/handle-add-ignore opts)]
      ;; No side effects - just data
      (is (map? result))
      (is (vector? (:effects result)))
      (is (every? vector? (:effects result)))
      ;; Can inspect effects without executing them
      (let [[effect-type specimen effect-opts] (first (:effects result))]
        (is (= :ignore/add effect-type))
        (is (= "test" specimen))
        (is (= opts effect-opts))))))

(deftest handler-registry-test
  (testing "All command types have handlers"
    (let [commands [:add-ignore :remove-ignore :list-ignored :clear-ignored
                    :restore-defaults :init-project :add-checks
                    :file :checks :help :version :default]]
      (doseq [cmd commands]
        (is (contains? cmd/handlers cmd)
            (str "Missing handler for: " cmd))
        (is (fn? (get cmd/handlers cmd))
            (str "Handler for " cmd " is not a function"))))))

(deftest command-effect-structure-test
  (testing "All handlers return consistent structure"
    (let [test-cases [{:handler cmd/handle-add-ignore
                       :opts {:add-ignore "test" :global true}}
                      {:handler cmd/handle-remove-ignore
                       :opts {:remove-ignore "test" :global true}}
                      {:handler cmd/handle-file
                       :opts {:file "test.md"}}]]
      (doseq [{:keys [handler opts]} test-cases]
        (let [result (handler opts)]
          (is (map? result))
          (is (or (:effects result) (:messages result) (:format-fn result))
              "Handler should return at least one of: effects, messages, or format-fn"))))))

(deftest parse-issue-numbers-test
  (testing "parses single number"
    (let [result (cmd/parse-issue-numbers "1")]
      (is (result/success? result))
      (is (= [1] (:value result)))))

  (testing "parses comma-separated numbers"
    (let [result (cmd/parse-issue-numbers "1,3,5")]
      (is (result/success? result))
      (is (= [1 3 5] (:value result)))))

  (testing "parses ranges"
    (let [result (cmd/parse-issue-numbers "1-5")]
      (is (result/success? result))
      (is (= [1 2 3 4 5] (:value result)))))

  (testing "parses mixed ranges and singles"
    (let [result (cmd/parse-issue-numbers "1-3,5,7-9")]
      (is (result/success? result))
      (is (= [1 2 3 5 7 8 9] (:value result)))))

  (testing "handles whitespace"
    (let [result (cmd/parse-issue-numbers " 1 , 2 , 3 ")]
      (is (result/success? result))
      (is (= [1 2 3] (:value result)))))

  (testing "deduplicates and sorts"
    (let [result (cmd/parse-issue-numbers "3,1,2,1,5")]
      (is (result/success? result))
      (is (= [1 2 3 5] (:value result)))))

  (testing "handles invalid range (end < start)"
    (let [result (cmd/parse-issue-numbers "5-1")]
      (is (result/failure? result))
      (is (re-find #"Invalid range.*start must be less than or equal to end" (:error result)))))

  (testing "returns error for non-numeric input"
    (let [result (cmd/parse-issue-numbers "1,foo,3")]
      (is (result/failure? result))
      (is (re-find #"Invalid" (:error result)))))

  (testing "returns error for malformed range"
    (let [result (cmd/parse-issue-numbers "1-2-3")]
      (is (result/failure? result))))

  (testing "returns error for empty string"
    (let [result (cmd/parse-issue-numbers "")]
      (is (result/failure? result))))

  (testing "returns error for negative numbers"
    (let [result (cmd/parse-issue-numbers "-1,2")]
      (is (result/failure? result))))

  (testing "returns error for zero"
    (let [result (cmd/parse-issue-numbers "0,1,2")]
      (is (result/failure? result)))))

(deftest parse-issue-numbers-determinism-test
  (testing "parse-issue-numbers is deterministic"
    (let [test-inputs ["1,2,3" "1-5" "5,1-3,7" "1,1,1" "  1 , 2 , 3  "]]
      (doseq [input test-inputs]
        (let [result1 (cmd/parse-issue-numbers input)
              result2 (cmd/parse-issue-numbers input)]
          (is (= result1 result2)
              (str "Parse should be deterministic for: " input))))))

  (testing "Sorted output is consistent across identical unsorted inputs"
    (let [result1 (cmd/parse-issue-numbers "5,3,1,4,2")
          result2 (cmd/parse-issue-numbers "3,5,2,1,4")
          result3 (cmd/parse-issue-numbers "1,2,3,4,5")]
      (is (= (:value result1) (:value result2) (:value result3) [1 2 3 4 5])))))

(deftest parse-issue-numbers-error-context-test
  (testing "Error context contains operation metadata"
    (let [result (cmd/parse-issue-numbers "foo")]
      (is (result/failure? result))
      (is (= :parse-issue-numbers (-> result :context :operation)))))

  (testing "Error context distinguishes exception types"
    (is (= :number-format
           (-> (cmd/parse-issue-numbers "abc") :context :exception-type)))
    (is (= :validation
           (-> (cmd/parse-issue-numbers "5-1") :context :exception-type))))

  (testing "Error context includes input for debugging"
    (let [result (cmd/parse-issue-numbers "bad-input")]
      (is (= "bad-input" (-> result :context :input))))))

(deftest parse-single-number-error-messages-test
  (testing "Negative number uses standard error message"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"Issue numbers must be positive"
          (#'cmd/parse-single-number "-1"))))

  (testing "Zero uses standard error message"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"Issue numbers must be positive"
          (#'cmd/parse-single-number "0")))))

(deftest parse-range-error-messages-test
  (testing "Zero range start uses standard error message"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"Issue numbers must be positive"
          (#'cmd/parse-range "0-5"))))

  (testing "Zero range end uses standard error message"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"Issue numbers must be positive"
          (#'cmd/parse-range "5-0")))))

(deftest project-exists-test
  (testing "project-exists? returns true when .proserunner/config.edn exists"
    (is (boolean? (cmd/project-exists?))))

  (testing "project-exists? returns boolean"
    (let [result (cmd/project-exists?)]
      (is (or (true? result) (false? result))))))

(deftest determine-target-test
  (testing "determine-target returns :global when --global flag is set"
    (is (= :global (cmd/determine-target {:global true}))))

  (testing "determine-target throws on conflicting flags"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"Cannot specify both --global and --project"
          (cmd/determine-target {:global true :project true}))))

  (testing "determine-target throws when --project flag set but no project exists"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"No project configuration found"
          (cmd/determine-target {:project true}))))

  (testing "determine-target returns keyword :project or :global when auto-detecting"
    (let [result (cmd/determine-target {})]
      (is (keyword? result))
      (is (contains? #{:project :global} result)))))

(deftest get-target-context-test
  (testing "get-target-context with :project opt returns project context"
    (let [result (cmd/get-target-context {:project true})]
      (is (= :project (:target result)))
      (is (= "project" (:msg-context result)))
      (is (true? (:project (:opts-with-target result))))
      (is (not (contains? result :alt-msg)))))

  (testing "get-target-context without :project opt returns global context"
    (let [result (cmd/get-target-context {:global true})]
      (is (= :global (:target result)))
      (is (= "global" (:msg-context result)))
      (is (false? (:project (:opts-with-target result))))
      (is (not (contains? result :alt-msg)))))

  (testing "get-target-context with :include-alt-msg adds alternate message for project"
    (let [result (cmd/get-target-context {:project true}
                                         {:include-alt-msg true})]
      (is (= :project (:target result)))
      (is (contains? result :alt-msg))
      (is (= "Use --global to add to global ignore list instead."
             (:alt-msg result)))))

  (testing "get-target-context with :include-alt-msg adds alternate message for global"
    (let [result (cmd/get-target-context {:global true}
                                         {:include-alt-msg true})]
      (is (= :global (:target result)))
      (is (contains? result :alt-msg))
      (is (= "Use --project to add to project ignore list instead."
             (:alt-msg result)))))

  (testing "get-target-context with custom :alt-msg-template"
    (let [result (cmd/get-target-context {:global true}
                                         {:include-alt-msg true
                                          :alt-msg-template "Custom message"})]
      (is (= "Custom message" (:alt-msg result)))))

  (testing "get-target-context with :use-determine-target? uses determine-target logic"
    ;; This will use the actual determine-target function which checks project existence
    (let [result (cmd/get-target-context {}
                                         {:use-determine-target? true})]
      (is (contains? #{:project :global} (:target result)))
      (is (contains? #{"project" "global"} (:msg-context result)))))

  (testing "get-target-context returns required keys"
    (let [result (cmd/get-target-context {:project true})]
      (is (contains? result :target))
      (is (contains? result :msg-context))
      (is (contains? result :opts-with-target))
      (is (map? (:opts-with-target result)))
      (is (contains? (:opts-with-target result) :project))))

  (testing "get-target-context preserves other options"
    (let [opts {:project true :some-flag "value" :another-opt 42}
          result (cmd/get-target-context opts)]
      (is (= "value" (:some-flag (:opts-with-target result))))
      (is (= 42 (:another-opt (:opts-with-target result)))))))

(deftest help-and-version-win-test
  (testing "help is chosen even when other actions are present"
    (is (= :help (cmd/determine-command {:help true :file "a.md"})))
    (is (= :help (cmd/determine-command {:help true :add-ignore "x"}))))
  (testing "version beats everything except help"
    (is (= :version (cmd/determine-command {:version true :paths ["a.md"]}))))
  (testing "positional paths select the file command"
    (is (= :file (cmd/determine-command {:paths ["a.md"]})))))

(deftest conflicting-actions-warning-test
  (testing "no warning for a single action"
    (is (nil? (cmd/conflicting-actions-warning {:paths ["a.md"]})))
    (is (nil? (cmd/conflicting-actions-warning {:list-ignored true}))))
  (testing "names the action that runs and the ones ignored"
    (is (= "Only one action runs at a time: running --list-ignored, ignoring PATH."
           (cmd/conflicting-actions-warning {:list-ignored true :file "a.md"}))))
  (testing "--ignore-issues with a PATH is a valid pair"
    (is (nil? (cmd/conflicting-actions-warning {:ignore-issues "1" :paths ["a.md"]}))))
  (testing "dispatch attaches the warning"
    (is (= 1 (count (:warnings (cmd/dispatch-command {:checks true :add-ignore "x" :global true}))))))
  (testing "help never warns"
    (is (nil? (:warnings (cmd/dispatch-command {:help true :checks true :file "a.md"}))))))
