(ns proserunner.error-test
  "Tests for error handling utilities."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.string :as string]
            [proserunner.error :as error]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [capture-output]]))

(defn- capture-err
  [f]
  (let [w (java.io.StringWriter.)]
    (binding [*err* w] (f))
    (str w)))

;; Tests for existing exit-based functions

(deftest message-prints-errors-test
  (testing "message prints each error to stderr, then a pointer to --help"
    (let [{:keys [output]} (capture-output
                             #(capture-err (fn [] (error/message ["Error 1" "Error 2" "Error 3"]))))
          err (capture-err #(error/message ["Error 1" "Error 2" "Error 3"]))]
      (is (= "" output) "nothing goes to stdout")
      (is (string/includes? err "proserunner: Error 1"))
      (is (string/includes? err "proserunner: Error 2"))
      (is (string/includes? err "proserunner: Error 3"))
      (is (string/ends-with? err (str error/help-hint "\n"))))))

(deftest suggest-option-test
  (let [long-opts ["--file" "--output" "--help" "--version" "--quiet"]]
    (testing "suggests the closest option for a likely typo"
      (is (= "--file" (error/suggest-option "--flie" long-opts)))
      (is (= "--output" (error/suggest-option "--ouput" long-opts)))
      (is (= "--version" (error/suggest-option "--verison" long-opts))))
    (testing "returns nil when nothing is close"
      (is (nil? (error/suggest-option "--frobnicate" long-opts))))))

(deftest describe-test
  (testing "rewrites unknown options with a suggestion"
    (is (= ["unknown option \"--flie\". Did you mean --file?"]
           (error/describe ["Unknown option: \"--flie\""] ["--file" "--help"]))))
  (testing "leaves other errors alone"
    (is (= ["Failed to validate \"-f x\": file must exist."]
           (error/describe ["Failed to validate \"-f x\": file must exist."] ["--file"]))))
  (testing "no suggestion when nothing is close"
    (is (= ["unknown option \"--zzzzzz\"."]
           (error/describe ["Unknown option: \"--zzzzzz\""] ["--file"])))))

(deftest message-result-returns-failure-test
  (testing "message-result returns Failure with errors"
    (let [result (error/message-result ["Error 1" "Error 2"])]
      (is (result/failure? result))
      (is (string/includes? (:error result) "Error 1"))
      (is (string/includes? (:error result) "Error 2")))))

(deftest message-result-includes-context-test
  (testing "message-result includes error details in context"
    (let [errors ["Error 1" "Error 2"]
          result (error/message-result errors)]
      (is (result/failure? result))
      (is (= errors (:errors (:context result)))))))

(deftest inferior-input-result-returns-failure-test
  (testing "inferior-input-result returns Failure instead of exiting"
    (let [result (error/inferior-input-result ["Bad input"])]
      (is (result/failure? result))
      (is (string/includes? (:error result) "Invalid input")))))

(deftest inferior-input-result-includes-errors-in-context-test
  (testing "inferior-input-result includes all errors in context"
    (let [errors ["Error 1" "Error 2" "Error 3"]
          result (error/inferior-input-result errors)]
      (is (result/failure? result))
      (is (= errors (:errors (:context result)))))))

(deftest exit-result-returns-failure-test
  (testing "exit-result returns Failure with error message"
    (let [result (error/exit-result)]
      (is (result/failure? result))
      (is (string/includes? (:error result) "--help"))))

  (testing "exit-result with custom message returns Failure"
    (let [result (error/exit-result "custom error message")]
      (is (result/failure? result))
      ;; Note: sentence-dress capitalizes and adds period
      (is (string/includes? (:error result) "Custom error message")))))

(deftest exit-result-preserves-error-context-test
  (testing "exit-result can include context"
    (let [result (error/exit-result "Error" {:code 404 :source "test"})]
      (is (result/failure? result))
      (is (= 404 (:code (:context result))))
      (is (= "test" (:source (:context result)))))))
