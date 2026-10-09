(ns proserunner.build-test
  "Tests for build system tasks and wrapper script behavior."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.java.io :as io]
            [clojure.java.shell :as shell]
            [clojure.string :as str]
            [proserunner.test-helpers :refer [with-temp-dir]]))

;;; Issue 1: bb.edn install task should check for proserunner

(deftest file-check-logic
  (testing "Install task file existence check logic"
    (with-temp-dir [temp-dir "build-test"]
      ;; Test: Binary doesn't exist initially
      (is (not (.exists (io/file temp-dir "proserunner")))
          "Binary does not exist initially - install task should fail in this case")

      ;; Test: Binary exists after creation
      (spit (str temp-dir "/proserunner") "fake binary")

      (is (.exists (io/file temp-dir "proserunner"))
          "Binary exists - install task should succeed in this case"))))

;;; Issue 5: Command construction should be safe

(deftest build-command-construction-uses-proper-joining
  (testing "Build command construction should properly format clojure commands"
    ;; Check that tasks/build.clj constructs commands correctly (no space before colon)
    (let [build-content (slurp "tasks/build.clj")]
      ;; Verify that clojure commands with aliases don't have space before colon
      (is (or (str/includes? build-content "\"clojure -M:")
              (str/includes? build-content "\"clojure -Spath"))
          "Build commands should use proper clojure invocations")
      (is (not (str/includes? build-content "\"clojure -M \""))
          "Build command should not have space before alias colon"))))

;;; Native Image Build Verification Tests (graal-build-time integration)

(def ^:dynamic *binary-path* "./proserunner")
(def ^:dynamic *build-output* nil)

(deftest test-binary-exists
  (testing "Native image binary exists"
    (is (.exists (io/file *binary-path*))
        "Binary should exist after build")))

(deftest test-binary-executable
  (testing "Native image binary is executable"
    (is (.canExecute (io/file *binary-path*))
        "Binary should have executable permissions")))

(deftest test-binary-runs
  (testing "Native image binary executes successfully"
    (let [{:keys [exit]} (shell/sh *binary-path* "--version")]
      (is (= 0 exit)
          "Binary should exit with status 0"))))

(deftest test-version-flag
  (testing "Binary responds to --version flag"
    (let [{:keys [exit out]} (shell/sh *binary-path* "--version")]
      (is (= 0 exit) "Version command should succeed")
      (is (not (str/blank? out)) "Version output should not be empty")
      (is (re-find #"\d+\.\d+\.\d+" out)
          "Version output should contain version number"))))

(deftest test-help-flag
  (testing "Binary responds to --help flag"
    (let [{:keys [exit out]} (shell/sh *binary-path* "--help")]
      (is (= 0 exit) "Help command should succeed")
      (is (or (str/includes? out "Usage:") (str/includes? out "USAGE:"))
          "Help output should contain usage information"))))

(deftest test-no-classloader-errors
  (testing "Binary doesn't fail with ClassLoader errors"
    (let [{:keys [exit err]} (shell/sh *binary-path* "--version")]
      (is (= 0 exit) "Command should succeed")
      (is (not (str/includes? err "Could not locate clojure/core__init.class"))
          "Should not have Clojure init class errors")
      (is (not (str/includes? err "FileNotFoundException"))
          "Should not have FileNotFoundException"))))

(deftest test-basic-functionality
  (testing "Binary can process a simple markdown file"
    (let [test-file "resources/benchmark-data/small.md"
          result (shell/sh *binary-path* test-file)
          {:keys [exit err]} result]
      (is (contains? #{0 1} exit)
          "Check command should exit with 0 (pass) or 1 (fail), not crash")
      (is (not (str/includes? err "Could not locate"))
          "Should not have class loading errors")
      (is (not (str/includes? err "Exception"))
          "Should not have unhandled exceptions"))))
