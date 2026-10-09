(ns proserunner.command-pipeline-test
  "Integration tests for the full command pipeline.

  Tests the end-to-end flow: options → validate → dispatch → execute"
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.commands :as cmd]
            [proserunner.effects :as effects]
            [proserunner.result :as result]))

(defmacro silently
  "Suppresses stdout/stderr during test execution."
  [& body]
  `(binding [*out* (java.io.StringWriter.)
             *err* (java.io.StringWriter.)]
     ~@body))

;; Integration Tests: Full Pipeline
;; These tests verify the complete flow from options to effect execution

(deftest pipeline-help-command-test
  (testing "Help command flows through entire pipeline"
    (silently
      (let [opts {:help true}
            ;; Step 1: Validate options
            validation (cmd/validate-options opts)]
        (is (result/success? validation))

        ;; Step 2: Dispatch command
        (let [cmd-result (cmd/dispatch-command opts)]
          (is (= :help (:command cmd-result)))
          (is (some? (:effects cmd-result)))

          ;; Step 3: Execute effects
          (let [effect-result (effects/execute-command-result cmd-result)]
            (is (result/success? effect-result))))))))

(deftest pipeline-version-command-test
  (testing "Version command flows through entire pipeline"
    (silently
      (let [opts {:version true}
            validation (cmd/validate-options opts)]
        (is (result/success? validation))

        (let [cmd-result (cmd/dispatch-command opts)]
          (is (= :version (:command cmd-result)))

          (let [effect-result (effects/execute-command-result cmd-result)]
            (is (result/success? effect-result))))))))

(deftest pipeline-validation-failure-test
  (testing "Parallel files on its own is valid (it implies sequential lines)"
    (let [opts {:parallel-files true}
          validation (cmd/validate-options opts)]
      (is (result/success? validation))))

  (testing "Global and project flags conflict"
    (let [opts {:global true :project true}
          validation (cmd/validate-options opts)]
      (is (result/failure? validation))
      (is (re-find #"global.*project" (:error validation))))))

(deftest pipeline-valid-flags-test
  (testing "Valid parallel-files with sequential-lines"
    (let [opts {:parallel-files true :sequential-lines true}
          validation (cmd/validate-options opts)]
      (is (result/success? validation))
      (is (= opts (:value validation))))))

(deftest pipeline-command-determination-test
  (testing "Pipeline correctly determines different command types"
    (let [test-cases [{:opts {:help true}
                       :expected-cmd :help}
                      {:opts {:version true}
                       :expected-cmd :version}
                      {:opts {:list-ignored true}
                       :expected-cmd :list-ignored}
                      {:opts {:checks true}
                       :expected-cmd :checks}
                      {:opts {}
                       :expected-cmd :default}]]
      (doseq [{:keys [opts expected-cmd]} test-cases]
        (let [validation (cmd/validate-options opts)]
          (is (result/success? validation)
              (str "Validation failed for: " opts))

          (let [cmd-result (cmd/dispatch-command opts)]
            (is (= expected-cmd (:command cmd-result))
                (str "Wrong command for opts: " opts))))))))

(deftest pipeline-effect-structure-test
  (testing "Command dispatch produces valid effect structures"
    (let [test-cases [{:opts {:help true}
                       :expected-effect-type :help/print}
                      {:opts {:version true}
                       :expected-effect-type :version/print}
                      {:opts {:list-ignored true}
                       :expected-effect-type :ignore/list}]]
      (doseq [{:keys [opts expected-effect-type]} test-cases]
        (let [cmd-result (cmd/dispatch-command opts)
              effects (:effects cmd-result)]
          (is (vector? effects)
              "Effects should be a vector")
          (is (every? vector? effects)
              "Each effect should be a vector")
          (is (= expected-effect-type (-> effects first first))
              (str "Wrong effect type for opts: " opts)))))))

(deftest pipeline-effect-execution-test
  (testing "Effects execute and return Results"
    (silently
      (let [opts {:help true}
            cmd-result (cmd/dispatch-command opts)
            exec-result (effects/execute-command-result cmd-result)]
        (is (or (result/success? exec-result) (result/failure? exec-result))
            "Execution should return a Result")))))

(deftest pipeline-unknown-effect-handling-test
  (testing "Unknown effects are handled gracefully"
    (silently
      (let [cmd-result {:effects [[:unknown/bad-effect "arg"]]}
            exec-result (effects/execute-command-result cmd-result)]
        (is (result/failure? exec-result))
        (is (re-find #"Unknown effect type" (:error exec-result)))))))

(deftest pipeline-message-formatting-test
  (testing "Messages are properly formatted in command results"
    (let [opts {:add-ignore "test-specimen" :global true}
          cmd-result (cmd/dispatch-command opts)]
      (is (some? (:messages cmd-result))
          "Command with messages should include :messages")
      (is (vector? (:messages cmd-result))
          "Messages should be a vector")
      (is (every? string? (:messages cmd-result))
          "All messages should be strings"))))

(deftest pipeline-format-function-test
  (testing "Format functions are properly included"
    (let [opts {:list-ignored true}
          cmd-result (cmd/dispatch-command opts)]
      (is (fn? (:format-fn cmd-result))
          "List command should include format function"))))

(deftest pipeline-end-to-end-success-test
  (testing "Complete successful pipeline: validate → dispatch → execute"
    (silently
      (let [opts {:version true}]
        ;; Full pipeline
        (-> opts
            cmd/validate-options
            (result/bind (fn [valid-opts]
                           (result/ok (cmd/dispatch-command valid-opts))))
            (result/bind (fn [cmd-result]
                           (effects/execute-command-result cmd-result)))
            (#(do
                (is (result/success? %))
                %)))))))

(deftest pipeline-end-to-end-failure-test
  (testing "Complete failing pipeline: validation fails early"
    (let [opts {:global true :project true}]
      ;; Full pipeline should fail at validation
      (-> opts
          cmd/validate-options
          (#(do
              (is (result/failure? %))
              (is (re-find #"global.*project" (:error %)))
              %))))))

(deftest pipeline-effect-short-circuit-test
  (testing "Multiple effects execute in sequence until failure"
    (silently
      (let [cmd-result {:effects [[:version/print]
                                  [:unknown/bad-effect]
                                  [:help/print {}]]}
            exec-result (effects/execute-command-result cmd-result)]
        ;; Should fail on the unknown effect
        (is (result/failure? exec-result))
        (is (re-find #"Unknown effect type" (:error exec-result)))))))

(deftest pipeline-idempotency-test
  (testing "Running the same pipeline twice produces same results"
    (let [opts {:version true}
          run-pipeline (fn []
                         (let [_validation (cmd/validate-options opts)
                               cmd-result (cmd/dispatch-command opts)]
                           cmd-result))
          result1 (run-pipeline)
          result2 (run-pipeline)]
      ;; Command results should be identical (pure functions)
      (is (= result1 result2)))))

(deftest pipeline-default-command-test
  (testing "Empty options trigger default command"
    (silently
      (let [opts {}
            cmd-result (cmd/dispatch-command opts)]
        (is (= :default (:command cmd-result)))
        (is (some? (:effects cmd-result)))
        (let [[effect-type] (first (:effects cmd-result))]
          (is (= :help/print-concise effect-type)))))))
