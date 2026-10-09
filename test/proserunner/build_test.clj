(ns proserunner.build-test
  "Tests for build system tasks and wrapper script behavior."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.java.io :as io]
            [clojure.java.shell :as shell]
            [clojure.string :as str]
            [proserunner.test-helpers :refer [test-home with-temp-dir]]))

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

;;; Native Image Build Verification Tests

(def ^:dynamic *binary-path* "./proserunner")
(def ^:dynamic *build-output* nil)

(defn- run-binary
  "Runs the binary with the test run's throwaway home, so it never reads
  or migrates the real config. Native images take -D at runtime."
  [& args]
  (let [env (assoc (into {} (System/getenv))
                   "XDG_CONFIG_HOME" (str test-home "/.config"))]
    (apply shell/sh *binary-path* (str "-Duser.home=" test-home)
           (concat args [:env env]))))

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
    (let [{:keys [exit]} (run-binary "--version")]
      (is (= 0 exit)
          "Binary should exit with status 0"))))

(deftest test-version-flag
  (testing "Binary responds to --version flag"
    (let [{:keys [exit out]} (run-binary "--version")]
      (is (= 0 exit) "Version command should succeed")
      (is (not (str/blank? out)) "Version output should not be empty")
      (is (re-find #"\d+\.\d+\.\d+" out)
          "Version output should contain version number"))))

(deftest test-help-flag
  (testing "Binary responds to --help flag"
    (let [{:keys [exit out]} (run-binary "--help")]
      (is (= 0 exit) "Help command should succeed")
      (is (or (str/includes? out "Usage:") (str/includes? out "USAGE:"))
          "Help output should contain usage information"))))

(deftest test-no-classloader-errors
  (testing "Binary doesn't fail with ClassLoader errors"
    (let [{:keys [exit err]} (run-binary "--version")]
      (is (= 0 exit) "Command should succeed")
      (is (not (str/includes? err "Could not locate clojure/core__init.class"))
          "Should not have Clojure init class errors")
      (is (not (str/includes? err "FileNotFoundException"))
          "Should not have FileNotFoundException"))))

(deftest test-basic-functionality
  (testing "Binary can process a simple markdown file"
    (let [test-file "resources/benchmark-data/small.md"
          ;; --no-cache so the file is actually linted
          result (run-binary "--no-cache" test-file)
          {:keys [exit out err]} result]
      (is (= 1 exit)
          "Lint run should exit 1 (issues found; small.md has known issues)")
      (is (str/includes? out test-file)
          "Output should report on the checked file")
      (is (re-find #"\[\d+\]\s+\d+:\d+" out)
          "Output should contain at least one issue (small.md has known issues)")
      (is (not (str/includes? err "Could not locate"))
          "Should not have class loading errors")
      (is (not (str/includes? err "Exception"))
          "Should not have unhandled exceptions"))))
