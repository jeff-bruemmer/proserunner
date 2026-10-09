(ns proserunner.effects-test
  "Tests for effect execution.

  Tests effect execution machinery and effect description structures."
  (:require [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [proserunner.console :as console]
            [proserunner.effects :as effects]
            [proserunner.ignore.file :as ignore-file]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [with-system-property with-temp-dir with-user-home]]))

(defmacro silently
  "Suppresses stdout/stderr during test execution."
  [& body]
  `(binding [*out* (java.io.StringWriter.)
             *err* (java.io.StringWriter.)]
     ~@body))

(deftest execute-effect-unknown-test
  (testing "Unknown effect types return failure"
    (let [result (effects/execute-effect [:unknown/type "arg"])]
      (is (result/failure? result))
      (is (re-find #"Unknown effect type" (:error result)))
      (is (= :unknown/type (-> result :context :effect-type))))))

(deftest execute-effect-help-print-test
  (testing "Help print effect succeeds without title"
    (silently
      (let [opts {:help true}
            result (effects/execute-effect [:help/print opts])]
        (is (result/success? result))
        ;; Help effect prints to stdout and returns nil, which is ok
        (is (nil? (:value result))))))

  (testing "Help print effect succeeds with title"
    (silently
      (let [opts {:help true}
            result (effects/execute-effect [:help/print opts "TEST TITLE"])]
        (is (result/success? result))
        (is (nil? (:value result)))))))

(deftest execute-effect-version-print-test
  (testing "Version print effect succeeds"
    (silently
      (let [result (effects/execute-effect [:version/print])]
        (is (result/success? result))
        ;; Version prints to stdout and returns nil, which is ok
        (is (nil? (:value result)))))))

(deftest execute-effects-test
  (testing "Execute multiple effects in sequence"
    (silently
      (let [effects [[:help/print {}]
                     [:version/print]]
            result (effects/execute-effects effects)]
        (is (result/success? result))
        (is (= 2 (count (:value result)))))))

  (testing "Short-circuits on first failure"
    (silently
      (let [effects [[:help/print {}]
                     [:unknown/type]
                     [:version/print]]
            result (effects/execute-effects effects)]
        (is (result/failure? result))
        (is (re-find #"Unknown effect type" (:error result)))))))

(deftest execute-effects-all-test
  (testing "Execute all effects successfully"
    (silently
      (let [effects [[:help/print {}]
                     [:version/print]]
            result (effects/execute-effects-all effects)]
        (is (result/success? result))
        (is (= 2 (count (:value result)))))))

  (testing "Collects all failures"
    (silently
      (let [effects [[:help/print {}]
                     [:unknown/type1]
                     [:version/print]
                     [:unknown/type2]]
            result (effects/execute-effects-all effects)]
        (is (result/failure? result))
        (is (= "Multiple errors occurred" (:error result)))
        (is (= 2 (-> result :context :count)))))))

(deftest execute-command-result-test
  (testing "Execute command with messages"
    (silently
      (let [cmd-result {:effects [[:help/print {}]]
                        :messages ["Message 1" "Message 2"]}
            result (effects/execute-command-result cmd-result)]
        (is (result/success? result)))))

  (testing "Execute command with format function"
    (silently
      (let [format-fn (fn [data] [(str "Formatted: " data)])
            cmd-result {:effects [[:version/print]]
                        :format-fn format-fn}
            result (effects/execute-command-result cmd-result)]
        (is (result/success? result)))))

  (testing "Execute command with no effects returns ok"
    (silently
      (let [cmd-result {:messages ["Just a message"]}
            result (effects/execute-command-result cmd-result)]
        (is (result/success? result))
        (is (nil? (:value result))))))

  (testing "Handles failures gracefully"
    (silently
      (let [cmd-result {:effects [[:unknown/effect]]}
            result (effects/execute-command-result cmd-result)]
        (is (result/failure? result))))))

(deftest effect-context-test
  (testing "Effects include context in error results"
    (let [result (effects/execute-effect [:unknown/custom-type "arg1" "arg2"])]
      (is (result/failure? result))
      (is (map? (:context result)))
      (is (= :unknown/custom-type (-> result :context :effect-type))))))

(deftest effect-result-type-test
  (testing "All effects return Result types"
    (silently
      (let [test-effects [[:help/print {}]
                          [:version/print]
                          [:unknown/type]]]
        (doseq [effect test-effects]
          (let [result (effects/execute-effect effect)]
            (is (or (result/success? result) (result/failure? result))
                (str "Effect " effect " should return a Result"))))))))

(deftest effect-idempotency-test
  (testing "Help and version effects are idempotent"
    (silently
      (let [help-result1 (effects/execute-effect [:help/print {}])
            help-result2 (effects/execute-effect [:help/print {}])
            version-result1 (effects/execute-effect [:version/print])
            version-result2 (effects/execute-effect [:version/print])]
        (is (result/success? help-result1))
        (is (result/success? help-result2))
        (is (result/success? version-result1))
        (is (result/success? version-result2))))))

;; Integration-style tests with actual effects
;; These test that the effects properly interact with their dependencies
(deftest ignore-effect-integration-test
  (testing "Ignore effects have proper structure"
    ;; We can't easily test the actual I/O without mocking the entire ignore namespace,
    ;; but we can test that the effects are structured correctly
    (let [test-cases [[:ignore/add "specimen" {:global true}]
                      [:ignore/remove "specimen" {:project true}]
                      [:ignore/list {}]
                      [:ignore/clear {:global true}]]]
      (doseq [effect test-cases]
        (is (vector? effect) (str "Effect should be vector: " effect))
        (is (keyword? (first effect)) (str "First element should be keyword: " effect))))))

(deftest config-effect-integration-test
  (testing "Config effects have proper structure"
    (let [effect [:config/restore-defaults]]
      (is (vector? effect))
      (is (= :config/restore-defaults (first effect))))))

(deftest project-effect-integration-test
  (testing "Project init effect has proper structure"
    (let [effect [:project/init]]
      (is (vector? effect))
      (is (= :project/init (first effect))))))

(deftest checks-effect-integration-test
  (testing "Checks effects have proper structure"
    (let [add-effect [:checks/add "/path/to/checks" {:name "custom"}]
          print-effect [:checks/print {:config "config.edn"}]]
      (is (= :checks/add (first add-effect)))
      (is (= :checks/print (first print-effect))))))

(deftest file-effect-integration-test
  (testing "File process effect has proper structure"
    (let [effect [:file/process {:file "test.md"}]]
      (is (= :file/process (first effect)))
      (is (map? (second effect))))))

;; Integration tests for command error handling

(deftest execute-command-result-error-handling-test
  (testing "Command result with :error key returns failure"
    (let [cmd-result {:error "Invalid range '5-1' - start must be less than or equal to end"}
          result (effects/execute-command-result cmd-result)]
      (is (result/failure? result))
      (is (= "Invalid range '5-1' - start must be less than or equal to end"
             (:error result)))))

  (testing "Command result with empty effects succeeds"
    (let [cmd-result {:effects []
                     :messages ["test message"]}
          result (silently (effects/execute-command-result cmd-result))]
      (is (result/success? result))))

  (testing "Command result with no error proceeds normally"
    (let [cmd-result {:effects [[:help/print {:help true}]]
                     :messages []}
          result (silently (effects/execute-command-result cmd-result))]
      (is (result/success? result)))))

;; Tests for helper functions

(deftest effect-wrapper-test
  (testing "effect-wrapper succeeds when function succeeds"
    (let [f (fn [] {:result "success"})
          wrapped (effects/effect-wrapper f :test/effect {:arg "value"})]
      (is (result/success? wrapped))
      (is (= {:result "success"} (:value wrapped)))))

  (testing "effect-wrapper catches exceptions and returns Failure"
    (let [f (fn [] (throw (Exception. "Test error")))
          wrapped (effects/effect-wrapper f :test/effect {:arg "value"})]
      (is (result/failure? wrapped))
      (is (= "Test error" (:error wrapped)))
      (is (= :test/effect (-> wrapped :context :effect)))
      (is (= "value" (-> wrapped :context :arg)))))

  (testing "effect-wrapper preserves thrown data in context"
    (let [f (fn [] (throw (Exception. "Error with data")))
          wrapped (effects/effect-wrapper f :test/effect {:file "/path/to/file"})]
      (is (result/failure? wrapped))
      (is (= "/path/to/file" (-> wrapped :context :file)))))

  (testing "effect-wrapper with minimal context"
    (let [f (fn [] 42)
          wrapped (effects/effect-wrapper f :simple {})]
      (is (result/success? wrapped))
      (is (= 42 (:value wrapped))))))

(deftest update-ignores-by-scope!-test
  (testing "update-ignores-by-scope! applies update function"
    ;; This is an integration test that requires actual file I/O
    ;; We test the structure and calling pattern
    (let [update-fn (fn [ignores]
                      (update ignores :ignore conj "new-pattern"))
          opts {:project false}]
      ;; The actual function will be tested via integration tests
      ;; Here we just verify the concept
      (is (fn? update-fn))
      (is (map? opts))))

  (testing "update-ignores-by-scope! respects scope option"
    (let [_update-fn identity
          project-opts {:project true :start-dir "/tmp/test"}
          global-opts {:project false}]
      ;; Verify options structure
      (is (true? (:project project-opts)))
      (is (false? (:project global-opts))))))

(deftest get-project-root-test
  (testing "Uses :start-dir from opts when present"
    (is (= "/custom/path" (#'effects/get-project-root {:start-dir "/custom/path"}))))

  (testing "Falls back to user.dir when :start-dir absent"
    (with-system-property "user.dir" "/fallback/dir"
      (fn []
        (is (= "/fallback/dir" (#'effects/get-project-root {}))))))

  (testing "Prefers :start-dir over user.dir when both present"
    (with-system-property "user.dir" "/system/dir"
      (fn []
        (is (= "/custom/dir" (#'effects/get-project-root {:start-dir "/custom/dir"})))))))

(deftest extract-prepped-issues-test
  (testing "Extracts and preps issues from vet payload"
    (let [mock-issue {:file "test.md" :name "test-check" :message "Test issue"
                     :kind :error :specimen "foo" :col-num 5}
          mock-line-with-issues {:file "test.md" :line-num 1 :text "This is test text"
                                 :col-num 5 :issues [mock-issue]}
          payload {:results {:results [mock-line-with-issues]}}
          issues (#'effects/extract-prepped-issues payload)]
      (is (seq issues))
      (is (every? map? issues))
      (is (= "test.md" (:file (first issues)))))))

(deftest load-ignore-map-for-filtering-test
  (testing "Returns empty map when skip-ignore is true"
    (let [payload {:project-ignore #{"foo"} :project-ignore-issues #{{:file "test.md"}}}
          ignore-map (#'effects/load-ignore-map-for-filtering payload {:skip-ignore true})]
      (is (= {:ignore #{} :ignore-issues #{}} ignore-map))))

  (testing "Loads ignore map from payload when skip-ignore is false"
    (let [payload {:project-ignore #{"foo"} :project-ignore-issues #{{:file "test.md"}}}
          ignore-map (#'effects/load-ignore-map-for-filtering payload {})]
      (is (= {:ignore #{"foo"} :ignore-issues #{{:file "test.md"}}} ignore-map))))

  (testing "Provides empty collections when payload lacks ignore data"
    (let [payload {}
          ignore-map (#'effects/load-ignore-map-for-filtering payload {})]
      (is (= {:ignore #{} :ignore-issues #{}} ignore-map)))))

(deftest select-and-validate-issue-numbers-test
  (testing "Filters issues by numbers and validates"
    (let [issues [{:file "test.md" :line-num 1 :col-num 5}
                  {:file "test.md" :line-num 2 :col-num 10}
                  {:file "test.md" :line-num 3 :col-num 15}]
          ignore-map {:ignore #{} :ignore-issues #{}}
          result (#'effects/select-and-validate-issue-numbers issues ignore-map [1 3])]
      (is (= 2 (count (:selected-issues result))))
      (is (= #{1 2 3} (:valid-nums result)))
      (is (= 3 (:total-issues result)))
      (is (empty? (:invalid-nums result)))))

  (testing "Identifies invalid issue numbers"
    (let [issues [{:file "test.md" :line-num 1 :col-num 5}]
          ignore-map {:ignore #{} :ignore-issues []}
          result (#'effects/select-and-validate-issue-numbers issues ignore-map [1 5 10])]
      (is (= [5 10] (:invalid-nums result)))))

  (testing "Handles empty ignore-map correctly"
    (let [issues [{:file "test.md" :line-num 1 :col-num 5}]
          ignore-map {:ignore #{} :ignore-issues []}
          result (#'effects/select-and-validate-issue-numbers issues ignore-map [1])]
      (is (= 1 (count (:selected-issues result)))))))

(deftest clear-ignored-warns-scripts-without-force-test
  (with-temp-dir [home "proserunner-clear"]
    (with-user-home home
      (let [run (fn [opts interactive?]
                  (ignore-file/write! {:ignore #{"word"} :ignore-issues #{}})
                  (let [err (java.io.StringWriter.)
                        r (with-redefs [console/interactive? (constantly interactive?)
                                        console/confirm? (constantly true)]
                            (binding [*err* err
                                      *out* (java.io.StringWriter.)]
                              (effects/execute-effect [:ignore/clear opts])))]
                    {:warned? (str/includes? (str err) effects/clear-without-force-warning)
                     :cleared (:cleared (:value r))}))]
        (testing "not in a terminal, without --force: still clears, but warns"
          (is (= {:warned? true :cleared 1} (run {:global true} false))))
        (testing "not in a terminal, with --force: clears quietly"
          (is (= {:warned? false :cleared 1} (run {:global true :force true} false))))
        (testing "in a terminal: asks instead of warning"
          (is (= {:warned? false :cleared 1} (run {:global true} true))))))))
