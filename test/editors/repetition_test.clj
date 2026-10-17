(ns editors.repetition-test
  (:require [proserunner
             [checks :as checks]
             [text :as text]]
            [clojure.test :as t :refer [deftest is]]
            [editors.repetition :as r]))

(def error-line (text/->Line
                 "resources"
                 "There is is something wrong with this this sentence."
                 42
                 false
                 false
                 false
                 []))

(def handsome-line (text/->Line
                    "resources"
                    "This sentence looks handsome."
                    42
                    false
                    false
                    false
                    []))

(def repetition (checks/map->Check {:name "Consecutive repeat words"
                                    :specimens []
                                    :message "Consecutive repeat words"
                                    :kind "repetition"
                                    :explanation "explanation"
                                    :recommendations []}))

(deftest existence
  (is (true? (:issue? (r/proofread error-line repetition))))
  (is (false? (:issue? (r/proofread handsome-line repetition)))))

(deftest repetition-columns
  (let [line #(text/->Line "resources" % 1 false false false [])
        found #(map (juxt :specimen :col-num) (:issues (r/proofread (line %) repetition)))]
    (is (= [["is is" 5]] (found "This is is fine."))
        "the repeat inside \"This is\" isn't mistaken for it")
    (is (= [["is is" 6] ["this this" 33]] (found "There is is something wrong with this this sentence.")))
    (is (= [["the  the" 0]] (found "the  the end")) "any whitespace between")
    (is (empty? (found "That, that is fine.")) "punctuation between isn't a repeat")))
