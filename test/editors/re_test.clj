(ns editors.re-test
  (:require [editors.re :as re]
            [proserunner.text :as text]
            [proserunner.checks :as checks]
            [clojure.test :as t :refer [deftest is]]))

(def error-line (text/->Line
                 "resources"
                 "This sentence is handsome."
                 42
                 false
                 false
                 false
                 []))

(def error-line-double (text/->Line
                        "resources"
                        "This sentence is handsome and too handsome."
                        42
                        false
                        false
                        false
                        []))

(def error-heading (text/->Line
                    "resources"
                    "## This heading is bad."
                    42
                    false
                    false
                    false
                    []))

(def decent-heading (text/->Line
                     "resources"
                     "### This heading is decent"
                     42
                     false
                     false
                     false
                     []))

(def innocuous-line (text/->Line
                     "resources"
                     "This sentence looks innocuous."
                     42
                     false
                     false
                     false
                     []))

(def re-checks (checks/map->Check {:name "re"
                                   :kind "re"
                                   :explanation "Raw regular expressions."
                                   :expressions [{:re "handsome" :message "This phrase is too handsome."}
                                                 {:re "(##+.*?\\.)" :message "Headings should end in a period"}]}))

(deftest re
  (is (true? (:issue? (re/proofread error-line re-checks))))
  (is (false? (:issue? (re/proofread innocuous-line re-checks))))
  (is (true? (:issue? (re/proofread error-heading re-checks))))
  (is (= 2 (count (:issues (re/proofread error-line-double re-checks))))))



(deftest re-columns
  (is (= [17 34] (map :col-num (:issues (re/proofread error-line-double re-checks)))))
  (is (= [0] (map :col-num (:issues (re/proofread error-heading re-checks))))))
