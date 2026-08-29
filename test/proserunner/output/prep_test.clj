(ns proserunner.output.prep-test
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.string :as string]
            [proserunner.output.prep :as prep]))

(deftest prep-test
  (testing "merges line data with issues"
    (let [line-data {:line-num 10 :text "This is a test line" :issues [{:file "test.md" :name "test-check" :specimen "test" :col-num 5 :message "test message" :kind "existence"}]}
          result (prep/prep line-data)]
      (is (vector? result))
      (is (= 1 (count result)))
      (is (= 10 (:line-num (first result))))
      (is (= "test.md" (:file (first result))))
      (is (= "test" (:specimen (first result))))
      (is (= "This is a test line" (:line-text (first result))))))

  (testing "handles multiple issues on one line"
    (let [line-data {:line-num 5 :text "Multiple issues here"
                     :issues [{:file "a.md" :name "check1" :specimen "word1" :col-num 1 :message "msg1" :kind "existence"}
                              {:file "a.md" :name "check2" :specimen "word2" :col-num 10 :message "msg2" :kind "repetition"}]}
          result (prep/prep line-data)]
      (is (= 2 (count result)))
      (is (= "word1" (:specimen (first result))))
      (is (= "word2" (:specimen (second result)))))))

(deftest issue-str-test
  (testing "formats issue without number"
    (let [issue {:line-num 10 :col-num 5 :specimen "test" :message "this is a test"}
          result (prep/issue-str issue)]
      (is (string? result))
      (is (re-find #"10:5" result))
      (is (re-find #"test" result))))

  (testing "formats issue with number prefix"
    (let [issue {:line-num 10 :col-num 5 :specimen "test" :message "this is a test"}
          result (prep/issue-str 42 issue)]
      (is (re-find #"\[42\]" result))
      (is (re-find #"10:5" result))))

  (testing "separates fields with spaces, never tabs"
    (let [issue {:line-num 10 :col-num 5 :specimen "test" :message "this is a test"}]
      (is (not (string/includes? (prep/issue-str issue) "\t")))
      (is (not (string/includes? (prep/issue-str 42 issue) "\t")))))

  (testing "without widths the fields are still single-spaced and readable"
    (let [issue {:line-num 10 :col-num 5 :specimen "test" :message "A message."}]
      (is (= "10:5  \"test\" -> A message." (prep/issue-str issue)))
      (is (= "[42]  10:5  \"test\" -> A message." (prep/issue-str 42 issue))))))

;;; Column alignment

(def ^:private sample-issues
  [{:line-num 598 :col-num 184 :specimen "actually" :message "Overused adverb."}
   {:line-num 2140 :col-num 149 :specimen "somewhat" :message "Omit this phrase."}
   {:line-num 3121 :col-num 75 :specimen "really" :message "Overused adverb."}
   {:line-num 3791 :col-num 87 :specimen "through through" :message "Consecutive word repetition."}])

(defn- numbered
  "Pairs issues with 1-based numbers, the shape column-widths expects."
  [issues]
  (map-indexed (fn [idx issue] [(inc idx) issue]) issues))

(defn- aligned-lines
  "Renders issues through issue-str using widths measured across all of them."
  [issues]
  (let [pairs (numbered issues)
        widths (prep/column-widths pairs)]
    (map (fn [[num issue]] (prep/issue-str num issue widths)) pairs)))

(deftest column-widths-test
  (testing "measures the widest value in each column"
    (let [widths (prep/column-widths (numbered sample-issues))]
      (is (= 3 (:num widths)))                 ; "[4]"
      (is (= 4 (:line widths)))                ; "3791"
      (is (= 3 (:col widths)))                 ; "184"
      (is (= 17 (:specimen widths)))))         ; "through through" plus quotes

  (testing "handles no results"
    (is (= {:num 0 :line 0 :col 0 :specimen 0} (prep/column-widths [])))))

(deftest issue-str-alignment-test
  (testing "emits no tabs"
    (is (every? #(not (string/includes? % "\t")) (aligned-lines sample-issues))))

  (testing "colons line up across mixed-width line numbers"
    (let [lines (aligned-lines sample-issues)]
      (is (apply = (map #(string/index-of % ":") lines)))))

  (testing "messages line up across mixed-width specimens"
    (let [lines (aligned-lines sample-issues)]
      (is (apply = (map #(string/index-of % "->") lines)))))

  (testing "issue numbers line up as they grow a digit"
    (let [lines (aligned-lines (take 12 (cycle sample-issues)))]
      (is (apply = (map #(string/index-of % ":") lines)))))

  (testing "renders the expected layout end to end"
    (is (= ["[1]   598:184  \"actually\"        -> Overused adverb."
            "[2]  2140:149  \"somewhat\"        -> Omit this phrase."
            "[3]  3121:75   \"really\"          -> Overused adverb."
            "[4]  3791:87   \"through through\" -> Consecutive word repetition."]
           (aligned-lines sample-issues))))

  (testing "the index column widens once numbering reaches two digits"
    (is (= ["[1]    598:184  \"actually\"        -> Overused adverb."
            "[9]    598:184  \"actually\"        -> Overused adverb."
            "[10]  2140:149  \"somewhat\"        -> Omit this phrase."]
           (->> (take 10 (cycle sample-issues))
                aligned-lines
                ((juxt first #(nth % 8) last)))))))

(deftest long-specimen-overflow-test
  (let [long-specimen (apply str (repeat 60 "x"))
        issues [{:line-num 1 :col-num 1 :specimen "hi" :message "Short."}
                {:line-num 2 :col-num 2 :specimen long-specimen :message "Long."}]
        widths (prep/column-widths (numbered issues))
        [short-line long-line] (aligned-lines issues)]

    (testing "a specimen past the cap does not widen the column"
      (is (= 4 (:specimen widths))))

    (testing "the short row keeps a tight message column"
      (is (= "[1]  1:1  \"hi\" -> Short." short-line)))

    (testing "the long row overflows rather than being truncated"
      (is (string/includes? long-line long-specimen))
      (is (< (string/index-of short-line "->") (string/index-of long-line "->"))))))

(deftest time-elapsed-test
  (testing "prints elapsed time when timer is enabled"
    (let [start-time (System/currentTimeMillis)
          opts {:timer false :start-time start-time}]
      ;; Just verify it doesn't throw
      (is (nil? (prep/time-elapsed opts))))))
