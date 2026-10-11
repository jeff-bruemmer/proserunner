(ns proserunner.ignore.core-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.ignore.core :as ignore]))

(deftest contextual-ignore?-test
  (testing "recognizes contextual ignore with file and specimen"
    (is (ignore/contextual-ignore? {:file "foo.md" :specimen "bar"})))

  (testing "recognizes contextual ignore with all fields"
    (is (ignore/contextual-ignore? {:file "foo.md" :specimen "bar" :line 10 :check "cliche"})))

  (testing "rejects simple string ignore"
    (is (not (ignore/contextual-ignore? "simple-ignore"))))

  (testing "rejects map without file"
    (is (not (ignore/contextual-ignore? {:specimen "bar"}))))

  (testing "rejects map without specimen"
    (is (not (ignore/contextual-ignore? {:file "foo.md"}))))

  (testing "rejects nil"
    (is (not (ignore/contextual-ignore? nil)))))

(deftest matches-simple-ignore?-test
  (testing "matches exact specimen"
    (is (ignore/matches-simple-ignore? "hello" {:specimen "hello"})))

  (testing "matches case-insensitively"
    (is (ignore/matches-simple-ignore? "Hello" {:specimen "HELLO"})))

  (testing "does not match different specimen"
    (is (not (ignore/matches-simple-ignore? "hello" {:specimen "goodbye"})))))

(deftest matches-contextual-ignore?-test
  (testing "matches exact file and specimen"
    (let [ignore {:file "test.md" :specimen "hello"}
          issue {:file "test.md" :specimen "hello" :line 10}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches case-insensitively for specimen"
    (let [ignore {:file "test.md" :specimen "Hello"}
          issue {:file "test.md" :specimen "HELLO" :line 10}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches without line specified"
    (let [ignore {:file "test.md" :specimen "hello"}
          issue {:file "test.md" :specimen "hello" :line 10}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches with specific line"
    (let [ignore {:file "test.md" :specimen "hello" :line 10}
          issue {:file "test.md" :specimen "hello" :line 10}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "does not match different line"
    (let [ignore {:file "test.md" :specimen "hello" :line 10}
          issue {:file "test.md" :specimen "hello" :line 20}]
      (is (not (ignore/matches-contextual-ignore? ignore issue)))))

  (testing "matches without check specified"
    (let [ignore {:file "test.md" :specimen "hello" :line 10}
          issue {:file "test.md" :specimen "hello" :line 10 :check "cliche"}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches with specific check"
    (let [ignore {:file "test.md" :specimen "hello" :line 10 :check "cliche"}
          issue {:file "test.md" :specimen "hello" :line 10 :check "cliche"}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "does not match different check"
    (let [ignore {:file "test.md" :specimen "hello" :line 10 :check "cliche"}
          issue {:file "test.md" :specimen "hello" :line 10 :check "repetition"}]
      (is (not (ignore/matches-contextual-ignore? ignore issue)))))

  (testing "does not match different file"
    (let [ignore {:file "test.md" :specimen "hello"}
          issue {:file "other.md" :specimen "hello" :line 10}]
      (is (not (ignore/matches-contextual-ignore? ignore issue)))))

  (testing "does not match different specimen"
    (let [ignore {:file "test.md" :specimen "hello"}
          issue {:file "test.md" :specimen "goodbye" :line 10}]
      (is (not (ignore/matches-contextual-ignore? ignore issue)))))

  (testing "handles :line-num field for backward compatibility"
    (let [ignore {:file "test.md" :specimen "hello" :line 10}
          issue {:file "test.md" :specimen "hello" :line-num 10}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "handles :name field for backward compatibility"
    (let [ignore {:file "test.md" :specimen "hello" :check "cliche"}
          issue {:file "test.md" :specimen "hello" :name "cliche"}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches with both :line and :line-num present (prefers :line-num)"
    (let [ignore {:file "test.md" :specimen "hello" :line-num 99}
          issue {:file "test.md" :specimen "hello" :line 10 :line-num 99}]
      (is (ignore/matches-contextual-ignore? ignore issue))))

  (testing "matches with both :check and :name present (prefers :name in issues, :check in ignores)"
    (let [ignore {:file "test.md" :specimen "hello" :check "cliche"}
          issue {:file "test.md" :specimen "hello" :check "other" :name "cliche"}]
      (is (ignore/matches-contextual-ignore? ignore issue)))))

(deftest should-ignore-issue?-test
  (testing "ignores issue matching simple ignore"
    (let [issue {:specimen "hello"}
          ignores ["hello" "world"]]
      (is (ignore/should-ignore-issue? issue ignores))))

  (testing "ignores issue matching contextual ignore"
    (let [issue {:file "test.md" :specimen "hello" :line 10}
          ignores [{:file "test.md" :specimen "hello" :line 10}]]
      (is (ignore/should-ignore-issue? issue ignores))))

  (testing "ignores with mixed simple and contextual ignores"
    (let [issue {:file "test.md" :specimen "hello" :line 10}
          ignores ["world" {:file "test.md" :specimen "hello"}]]
      (is (ignore/should-ignore-issue? issue ignores))))

  (testing "does not ignore unmatched issue"
    (let [issue {:file "test.md" :specimen "hello" :line 10}
          ignores ["world" {:file "other.md" :specimen "hello"}]]
      (is (not (ignore/should-ignore-issue? issue ignores)))))

  (testing "handles empty ignore list"
    (let [issue {:specimen "hello"}]
      (is (not (ignore/should-ignore-issue? issue [])))))

  (testing "handles nil ignore list"
    (let [issue {:specimen "hello"}]
      (is (not (ignore/should-ignore-issue? issue nil))))))

;;; Indexing tests

(deftest build-ignore-index-test
  (testing "indexes simple ignores as lowercase set"
    (let [ignores {:ignore #{"Hello" "World"} :ignore-issues []}
          index (ignore/build-ignore-index ignores)]
      (is (= #{"hello" "world"} (:simple index)))
      (is (empty? (:contextual index)))))

  (testing "groups contextual ignores by file"
    (let [ignores {:ignore #{} :ignore-issues [{:file "a.md" :line 10 :specimen "test"}]}
          index (ignore/build-ignore-index ignores)]
      (is (empty? (:simple index)))
      (is (contains? (:contextual index) "a.md"))))

  (testing "separates line-specific from file-wide"
    (let [ignores {:ignore #{}
                   :ignore-issues [{:file "a.md" :line 10 :specimen "line"}
                                   {:file "a.md" :specimen "file"}]}
          index (ignore/build-ignore-index ignores)]
      (is (seq (get-in index [:contextual "a.md" :by-line 10])))
      (is (seq (get-in index [:contextual "a.md" :file-wide])))))

  (testing "handles mixed simple and contextual"
    (let [ignores {:ignore #{"simple"} :ignore-issues [{:file "a.md" :specimen "ctx"}]}
          index (ignore/build-ignore-index ignores)]
      (is (contains? (:simple index) "simple"))
      (is (contains? (:contextual index) "a.md"))))

  (testing "handles multiple files"
    (let [ignores {:ignore #{}
                   :ignore-issues [{:file "a.md" :specimen "test"}
                                   {:file "b.md" :specimen "test"}]}
          index (ignore/build-ignore-index ignores)]
      (is (contains? (:contextual index) "a.md"))
      (is (contains? (:contextual index) "b.md")))))

;;; Performance tests

(deftest filter-issues-performance-optimized
  (testing "improved performance with indexed lookups"
    (let [issues (vec (for [i (range 1000)]
                        {:file "test.md"
                         :line i
                         :specimen (str "word" i)
                         :check "test"}))
          simple-ignores (set (map str (range 50)))
          contextual-ignores (vec (for [i (range 50 100)]
                                   {:file "test.md" :line i :specimen (str "word" i)}))
          ignores {:ignore simple-ignores :ignore-issues contextual-ignores}
          start (System/nanoTime)
          result (ignore/filter-issues issues ignores)
          elapsed-ms (/ (- (System/nanoTime) start) 1000000.0)]
      (is (vector? result))
      (is (< (count result) (count issues)) "Some issues should be filtered")
      ;; Should complete in reasonable time (50ms is generous, typically much faster)
      (is (< elapsed-ms 50) "Optimized version should be fast"))))

;; Tests for pure functions

(deftest add-specimen-test
  (testing "adds string specimen to :ignore set"
    (let [ignores {:ignore #{"foo"} :ignore-issues #{}}
          result (ignore/add-specimen ignores "bar")]
      (is (= #{"foo" "bar"} (:ignore result)))
      (is (= #{} (:ignore-issues result)))))

  (testing "adds map specimen to :ignore-issues set"
    (let [ignores {:ignore #{} :ignore-issues #{{:file "a.md" :specimen "old"}}}
          new-issue {:file "b.md" :specimen "new"}
          result (ignore/add-specimen ignores new-issue)]
      (is (= #{} (:ignore result)))
      (is (= #{{:file "a.md" :specimen "old"}
               {:file "b.md" :specimen "new"}}
             (:ignore-issues result)))))

  (testing "preserves existing ignores when adding string"
    (let [ignores {:ignore #{"existing"} :ignore-issues #{{:file "x.md" :specimen "ctx"}}}
          result (ignore/add-specimen ignores "new")]
      (is (= #{"existing" "new"} (:ignore result)))
      (is (= #{{:file "x.md" :specimen "ctx"}} (:ignore-issues result)))))

  (testing "preserves existing ignores when adding map"
    (let [ignores {:ignore #{"existing"} :ignore-issues #{{:file "x.md" :specimen "ctx"}}}
          result (ignore/add-specimen ignores {:file "y.md" :specimen "new"})]
      (is (= #{"existing"} (:ignore result)))
      (is (= #{{:file "x.md" :specimen "ctx"}
               {:file "y.md" :specimen "new"}}
             (:ignore-issues result))))))

(deftest add-specimen-deduplication-test
  (testing "adding duplicate contextual ignore doesn't create duplicate"
    (let [ignores {:ignore #{} :ignore-issues #{}}
          issue {:file "a.md" :specimen "test"}
          result (-> ignores
                     (ignore/add-specimen issue)
                     (ignore/add-specimen issue))]
      (is (= 1 (count (:ignore-issues result))))
      (is (= #{issue} (:ignore-issues result)))))

  (testing "adding duplicate simple ignore doesn't create duplicate"
    (let [ignores {:ignore #{} :ignore-issues #{}}
          result (-> ignores
                     (ignore/add-specimen "word")
                     (ignore/add-specimen "word"))]
      (is (= 1 (count (:ignore result))))
      (is (= #{"word"} (:ignore result))))))

(deftest remove-specimen-test
  (testing "removes string specimen from :ignore set"
    (let [ignores {:ignore #{"foo" "bar"} :ignore-issues #{}}
          result (ignore/remove-specimen ignores "bar")]
      (is (= #{"foo"} (:ignore result)))
      (is (= #{} (:ignore-issues result)))))

  (testing "removes map specimen from :ignore-issues set"
    (let [issue1 {:file "a.md" :specimen "keep"}
          issue2 {:file "b.md" :specimen "remove"}
          ignores {:ignore #{} :ignore-issues #{issue1 issue2}}
          result (ignore/remove-specimen ignores issue2)]
      (is (= #{} (:ignore result)))
      (is (= #{issue1} (:ignore-issues result)))))

  (testing "preserves other ignores when removing string"
    (let [ignores {:ignore #{"keep" "remove"} :ignore-issues #{{:file "x.md" :specimen "ctx"}}}
          result (ignore/remove-specimen ignores "remove")]
      (is (= #{"keep"} (:ignore result)))
      (is (= #{{:file "x.md" :specimen "ctx"}} (:ignore-issues result)))))

  (testing "preserves other ignores when removing map"
    (let [issue1 {:file "a.md" :specimen "keep"}
          issue2 {:file "b.md" :specimen "remove"}
          ignores {:ignore #{"string-ignore"} :ignore-issues #{issue1 issue2}}
          result (ignore/remove-specimen ignores issue2)]
      (is (= #{"string-ignore"} (:ignore result)))
      (is (= #{issue1} (:ignore-issues result)))))

  (testing "handles removal of non-existent string"
    (let [ignores {:ignore #{"foo"} :ignore-issues #{}}
          result (ignore/remove-specimen ignores "nonexistent")]
      (is (= #{"foo"} (:ignore result)))
      (is (= #{} (:ignore-issues result)))))

  (testing "handles removal of non-existent map"
    (let [ignores {:ignore #{} :ignore-issues #{{:file "a.md" :specimen "keep"}}}
          result (ignore/remove-specimen ignores {:file "b.md" :specimen "nonexistent"})]
      (is (= #{} (:ignore result)))
      (is (= #{{:file "a.md" :specimen "keep"}} (:ignore-issues result))))))

;; Tests for issue conversion

(deftest issue->ignore-entry-test
  (testing "converts to file-wide ignore with :file granularity"
    (let [issue {:file "test.md" :line-num 10 :specimen "word" :name "check"}
          result (ignore/issue->entry issue {:granularity :file})]
      (is (= {:file "test.md" :specimen "word"} result))))

  (testing "converts to line-specific ignore with :line granularity (default)"
    (let [issue {:file "test.md" :line-num 10 :specimen "word" :name "check"}
          result (ignore/issue->entry issue {:granularity :line})]
      (is (= {:file "test.md" :line-num 10 :specimen "word"} result))))

  (testing "converts to full ignore with :full granularity"
    (let [issue {:file "test.md" :line-num 10 :specimen "word" :name "check"}
          result (ignore/issue->entry issue {:granularity :full})]
      (is (= {:file "test.md" :line-num 10 :specimen "word" :check "check"} result))))

  (testing "defaults to :line granularity"
    (let [issue {:file "test.md" :line-num 10 :specimen "word"}
          result (ignore/issue->entry issue)]
      (is (= {:file "test.md" :line-num 10 :specimen "word"} result)))))

(deftest issues->ignore-entries-test
  (testing "converts multiple issues"
    (let [issues [{:file "a.md" :line 1 :specimen "x"}
                  {:file "a.md" :line 2 :specimen "y"}]
          result (ignore/issues->entries issues)]
      (is (= 2 (count result)))))

  (testing "deduplicates automatically with sets"
    (let [issues [{:file "a.md" :line 1 :specimen "x"}
                  {:file "a.md" :line 1 :specimen "x"}]
          result (ignore/issues->entries issues)]
      (is (= 1 (count result)))))

  (testing "returns set of entries"
    (let [issues [{:file "a.md" :line 1 :specimen "x"}
                  {:file "b.md" :line 2 :specimen "y"}]
          result (ignore/issues->entries issues)]
      (is (set? result))
      (is (= 2 (count result))))))
