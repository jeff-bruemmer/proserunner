(ns proserunner.core-test
  "Tests for core CLI entry point and command dispatch."
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.core :as core]
            [proserunner.effects]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [silently]]))

;; Note: Many error paths in core/reception call System/exit directly
;; and cannot be easily tested until we refactor to use result/result-or-exit.
;; These tests focus on successful execution paths.

(deftest reception-version-command-test
  (testing "reception returns options for --version command"
    (silently
      (let [options (core/reception ["--version"])]
        (is (some? options))
        (is (contains? options :version))
        (is (true? (:version options)))))))

(deftest reception-help-command-test
  (testing "reception handles --help command"
    (silently
      (let [options (core/reception ["--help"])]
        (is (some? options))
        (is (true? (:help options)))))))

(deftest reception-checks-command-test
  (testing "reception handles --checks command"
    (silently
      (let [options (core/reception ["--checks"])]
        (is (some? options))
        (is (true? (:checks options)))))))

(deftest reception-with-valid-file-test
  (testing "reception processes file argument when valid"
    ;; This tests that reception doesn't exit when given valid options
    ;; We mock the effect execution to avoid actually processing files
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [cmd]
                    ;; Return success for test
                    (result/ok {:command (:command cmd)}))]
      (silently
        (let [temp-file (java.io.File/createTempFile "test" ".md")]
          (try
            (spit (.getPath temp-file) "# Test file")
            (let [options (core/reception ["--file" (.getPath temp-file)])]
              (is (some? options))
              (is (= (.getPath temp-file) (:file options))))
            (finally
              (.delete temp-file))))))))

(deftest reception-returns-expanded-options-test
  (testing "reception returns expanded options with defaults"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok {}))]
      (silently
        (let [options (core/reception ["--version"])]
          ;; Should have default values expanded
          (is (contains? options :code-blocks))
          (is (contains? options :quoted-text))
          (is (contains? options :output))
          (is (false? (:code-blocks options)))
          (is (false? (:quoted-text options)))
          (is (= "group" (:output options))))))))

(defn- run-reception
  "Runs reception with stdout and stderr captured and the global config
  bootstrap stubbed. Returns {:options :out :err :bootstrapped?}."
  [args]
  (let [out (java.io.StringWriter.)
        err (java.io.StringWriter.)
        bootstrapped? (atom false)
        options (with-redefs [proserunner.config/ensure-global-config!
                              (fn [] (reset! bootstrapped? true))]
                  (binding [*out* out *err* err]
                    (core/reception args)))]
    {:options options :out (str out) :err (str err) :bootstrapped? @bootstrapped?}))

(deftest exit-code-test
  (testing "issues found exits 1"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok [{:issue-count 3 :file-count 1}]))]
      (is (= 1 (-> (run-reception ["README.md"]) :options :exit-code)))))

  (testing "no issues exits 0"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok [{:issue-count 0 :file-count 1}]))]
      (is (= 0 (-> (run-reception ["README.md"]) :options :exit-code)))))

  (testing "a failed run exits 2"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/err "boom"))]
      (is (= 2 (-> (run-reception ["README.md"]) :options :exit-code)))))

  (testing "conflicting flags exit 2 with the error on stderr only"
    (let [{:keys [options out err]} (run-reception ["--global" "--project" "--list-ignored"])]
      (is (= 2 (:exit-code options)))
      (is (= "" out))
      (is (re-find #"--global and --project" err))))

  (testing "unknown option exits 2 and suggests the closest flag"
    (let [{:keys [options out err]} (run-reception ["--flie" "README.md"])]
      (is (= 2 (:exit-code options)))
      (is (= "" out))
      (is (re-find #"Did you mean --file\?" err))
      (is (re-find #"--help" err))))

  (testing "invalid output format exits 2"
    (let [{:keys [options err]} (run-reception ["-o" "yaml" "README.md"])]
      (is (= 2 (:exit-code options)))
      (is (re-find #"must be one of" err))))

  (testing "missing positional path exits 2"
    (let [{:keys [options err]} (run-reception ["no-such-file.md"])]
      (is (= 2 (:exit-code options)))
      (is (re-find #"no-such-file\.md: no such file or directory" err))))

  (testing "uncaught ExceptionInfo becomes a one-line error, exit 2"
    (with-redefs [proserunner.commands/dispatch-command
                  (fn [_] (throw (ex-info "No project configuration found" {})))]
      (let [{:keys [options err]} (run-reception ["README.md"])]
        (is (= 2 (:exit-code options)))
        (is (re-find #"proserunner: No project configuration found" err))
        (is (not (re-find #"at proserunner\." err)) "no stack trace")))))

(deftest help-wins-test
  (testing "-h shows help even after other flags, without touching ~/.proserunner"
    (let [{:keys [options out bootstrapped?]} (run-reception ["--file" "README.md" "-h"])]
      (is (= 0 (:exit-code options)))
      (is (re-find #"USAGE" out))
      (is (false? bootstrapped?))))

  (testing "-h wins over parse errors"
    (let [{:keys [options out]} (run-reception ["--bogus" "-h"])]
      (is (= 0 (:exit-code options)))
      (is (re-find #"USAGE" out))))

  (testing "no arguments prints concise help"
    (let [{:keys [options out bootstrapped?]} (run-reception [])]
      (is (= 0 (:exit-code options)))
      (is (re-find #"Run 'proserunner --help'" out))
      (is (false? bootstrapped?)))))

(deftest positional-paths-test
  (testing "positional arguments and --file are collected into :paths"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok [{:issue-count 0}]))]
      (let [{:keys [options]} (run-reception ["--file" "README.md" "docs/usage.md" "LICENSE.txt"])]
        (is (= ["README.md" "docs/usage.md" "LICENSE.txt"] (:paths options)))))))

(deftest reception-with-parallel-and-sequential-test
  (testing "reception accepts --parallel-files with --sequential-lines (valid combination)"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok {}))]
      (silently
        (let [temp-file (java.io.File/createTempFile "test" ".md")]
          (try
            (spit (.getPath temp-file) "# Test")
            (let [options (core/reception ["--file" (.getPath temp-file)
                                           "--parallel-files"
                                           "--sequential-lines"])]
              (is (some? options))
              (is (true? (:parallel-files options)))
              (is (true? (:sequential-lines options))))
            (finally
              (.delete temp-file))))))))

(deftest reception-with-output-format-test
  (testing "reception accepts custom output format"
    (with-redefs [proserunner.effects/execute-command-result
                  (fn [_] (result/ok {}))]
      (silently
        (let [options (core/reception ["--output" "json" "--version"])]
          (is (some? options))
          (is (= "json" (:output options))))))))
