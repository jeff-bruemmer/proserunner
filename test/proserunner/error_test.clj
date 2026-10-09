(ns proserunner.error-test
  "Tests for error handling utilities."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.string :as string]
            [proserunner.error :as error]
            [proserunner.test-helpers :refer [capture-output]]))

(defn- capture-err
  [f]
  (let [w (java.io.StringWriter.)]
    (binding [*err* w] (f))
    (str w)))

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
