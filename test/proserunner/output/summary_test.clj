(ns proserunner.output.summary-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.output :as output]))

(deftest summary-test
  (testing "pluralizes files and issues"
    (is (= "Checked 1 file, found no issues." (output/summary 0 1)))
    (is (= "Checked 2 files, found 1 issue." (output/summary 1 2)))
    (is (= "Checked 3 files, found 5 issues." (output/summary 5 3)))))
