(ns editors.utilities-test
  "Tests for editor utilities including LRU cache."
  (:require [editors.utilities :as util]
            [proserunner.text :as text]
            [clojure.test :as t :refer [deftest is testing]]))

(deftest test-make-pattern
  (testing "Pattern creation with case sensitivity"
    (let [pattern (util/safe-make-pattern "test|example" false)]
      (is (instance? java.util.regex.Pattern pattern)))

    (let [pattern (util/safe-make-pattern "test|example" true)]
      (is (instance? java.util.regex.Pattern pattern)))))

(deftest test-seek
  (testing "Finding specimens in text"
    (let [text "This is a test of the test function."
          matches (util/seek text "test" false)]
      (is (vector? matches))
      (is (= 2 (count matches)))
      ;; Matches include boundary characters (space before word)
      (is (every? #(re-find #"test" %) matches))))

  (testing "Case-insensitive matching"
    (let [text "Test this TEST and test."
          matches (util/seek text "test" false)]
      ;; Should find at least 2 instances
      (is (>= (count matches) 2))))

  (testing "Case-sensitive matching"
    (let [text "Test this test."
          matches (util/seek text "test" true)]
      (is (= 1 (count matches)))))

  (testing "No matches returns empty vector"
    (let [text "This has no specimens."
          matches (util/seek text "hopefully" false)]
      (is (empty? matches)))))

(deftest test-add-issue
  (testing "Adding issue to line"
    (let [line (text/map->Line {:file "test.md"
                                :text "This is hopefully a test."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          result (util/add-issue {:line line
                                  :specimen "hopefully"
                                  :name "test-check"
                                  :kind "existence"
                                  :message "Test message"})]
      (is (:issue? result))
      (is (= 1 (count (:issues result))))
      (is (= "hopefully" (:specimen (first (:issues result)))))
      (is (number? (:col-num (first (:issues result)))))))

  (testing "No issue added when specimen not in text"
    (let [line (text/map->Line {:file "test.md"
                                :text "This is a test."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          result (util/add-issue {:line line
                                  :specimen "hopefully"
                                  :name "test-check"
                                  :kind "existence"
                                  :message "Test message"})]
      (is (false? (:issue? result)))
      (is (empty? (:issues result))))))

(deftest test-create-issue-collector
  (testing "Issue collector finds specimens"
    (let [collector (util/create-issue-collector false)
          line (text/map->Line {:file "test.md"
                                :text "This is hopefully a test."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          check {:file "test.md"
                 :kind "existence"
                 :specimens ["hopefully" "test"]
                 :message "Test message"
                 :name "test-check"}
          result (collector line check)]
      (is (:issue? result))
      (is (pos? (count (:issues result))))))

  (testing "Issue collector with empty specimens"
    (let [collector (util/create-issue-collector false)
          line (text/map->Line {:file "test.md"
                                :text "This is a test."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          check {:file "test.md"
                 :kind "existence"
                 :specimens []
                 :message "Test message"
                 :name "test-check"}
          result (collector line check)]
      (is (false? (:issue? result)))
      (is (empty? (:issues result))))))

(deftest test-column-numbering
  (testing "Column numbers point to the start of specimens"
    (let [text "You should utilize this hopefully dead artifact again."
          line (text/map->Line {:file "test.md"
                                :text text
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          recommender (util/create-recommender false)
          check {:name "test-recommender"
                 :kind "recommender"
                 :recommendations [{:avoid "utilize"
                                    :prefer "use"}
                                   {:avoid "hopefully"
                                    :prefer "perhaps"}]}
          result (recommender line check)]
      (is (:issue? result))
      (is (= 2 (count (:issues result))))
      ;; Find the utilize issue
      (let [utilize-issue (first (filter #(= "utilize" (:specimen %)) (:issues result)))
            hopefully-issue (first (filter #(= "hopefully" (:specimen %)) (:issues result)))]
        ;; "utilize" starts at position 11 (0-indexed)
        (is (= 11 (:col-num utilize-issue))
            (str "Expected 'utilize' at column 11, got " (:col-num utilize-issue)))
        ;; "hopefully" starts at position 24 (0-indexed)
        (is (= 24 (:col-num hopefully-issue))
            (str "Expected 'hopefully' at column 24, got " (:col-num hopefully-issue)))))))

(deftest test-create-recommender
  (testing "Recommender finds and suggests replacements"
    (let [recommender (util/create-recommender false)
          line (text/map->Line {:file "test.md"
                                :text "I would utilize this tool."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          check {:name "test-recommender"
                 :kind "recommender"
                 :recommendations [{:avoid "utilize"
                                    :prefer "use"}]}
          result (recommender line check)]
      (is (:issue? result))
      (is (pos? (count (:issues result))))
      (let [issue (first (:issues result))]
        ;; Specimen should be exactly "utilize" without boundary characters
        (is (= "utilize" (:specimen issue)))
        (is (re-find #"Prefer: use" (:message issue))))))

  (testing "Recommender with no matches"
    (let [recommender (util/create-recommender false)
          line (text/map->Line {:file "test.md"
                                :text "This is fine."
                                :line-num 1
                                :code? false
                                :quoted? false
                                :issue? false
                                :issues []})
          check {:name "test-recommender"
                 :kind "recommender"
                 :recommendations [{:avoid "utilize"
                                    :prefer "use"}]}
          result (recommender line check)]
      (is (false? (:issue? result)))
      (is (empty? (:issues result))))))

(deftest seek-after-punctuation
  (testing "specimens after punctuation are found"
    (doseq [text ["it is (very good)" "it is *very* good" "it is **very** good"
                  "it is 'very' good" "and/very" "so: very"]]
      (is (= ["very"] (util/seek text "very" false)) text)))
  (testing "specimens in markdown links, after #, -, or _ are skipped"
    (doseq [text ["[very](link)" "#very" "well-very" "_very_"]]
      (is (= [] (util/seek text "very" false)) text))))
