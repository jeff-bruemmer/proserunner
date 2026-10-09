(ns proserunner.output.summary-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.output :as output]))

(deftest summary-test
  (testing "pluralizes issues"
    (is (= "No issues found." (output/summary 0)))
    (is (= "Found 1 issue." (output/summary 1)))
    (is (= "Found 5 issues." (output/summary 5)))))
