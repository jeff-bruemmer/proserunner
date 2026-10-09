(ns proserunner.ignore.audit-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.ignore.audit :as audit]
            [clojure.java.io :as io]))

(deftest file-exists?-test
  (testing "returns true for existing file"
    ;; Test with a file we know exists (this test file itself)
    (let [this-file (io/file "test/proserunner/ignore/audit_test.clj")]
      (is (audit/file-exists? (.getPath this-file)))))

  (testing "returns false for non-existent file"
    (is (not (audit/file-exists? "/nonexistent/path/to/file.md"))))

  (testing "returns false for nil path"
    (is (not (audit/file-exists? nil)))))

(deftest audit-ignores-test
  (testing "identifies stale contextual ignores"
    (let [ignores {:ignore #{"simple"} ; simple ignores are always active
                   :ignore-issues [{:file "/nonexistent/file.md" :specimen "test"}
                                   {:file "test/proserunner/ignore/audit_test.clj" :specimen "existing"}]}
          result (audit/audit ignores)]
      (is (map? result))
      (is (contains? result :stale))
      (is (contains? result :active))
      ;; All simple ignores should be active
      (is (= #{"simple"} (get-in result [:active :ignore])))
      ;; Stale should have no simple ignores
      (is (= #{} (get-in result [:stale :ignore])))
      ;; Should identify the nonexistent file as stale
      (is (some #(= "/nonexistent/file.md" (:file %))
                (get-in result [:stale :ignore-issues])))
      ;; Should identify the existing file as active
      (is (some #(= "test/proserunner/ignore/audit_test.clj" (:file %))
                (get-in result [:active :ignore-issues])))))

  (testing "handles all active ignores"
    (let [ignores {:ignore #{"word"}
                   :ignore-issues [{:file "test/proserunner/ignore/audit_test.clj" :specimen "test"}]}
          result (audit/audit ignores)]
      (is (empty? (get-in result [:stale :ignore-issues])))
      (is (seq (get-in result [:active :ignore-issues])))))

  (testing "handles all stale ignores"
    (let [ignores {:ignore #{}
                   :ignore-issues [{:file "/nonexistent1.md" :specimen "a"}
                                   {:file "/nonexistent2.md" :specimen "b"}]}
          result (audit/audit ignores)]
      (is (= 2 (count (get-in result [:stale :ignore-issues]))))
      (is (empty? (get-in result [:active :ignore-issues])))))

  (testing "handles empty ignore list"
    (let [ignores {:ignore #{} :ignore-issues []}
          result (audit/audit ignores)]
      (is (empty? (get-in result [:stale :ignore-issues])))
      (is (empty? (get-in result [:active :ignore-issues]))))))

(deftest format-audit-report-test
  (testing "formats report with no stale ignores"
    (let [audit-result {:stale {:ignore #{} :ignore-issues []}
                        :active {:ignore #{"word"} :ignore-issues [{:file "a.md" :specimen "test"}]}}
          report (audit/format-report audit-result)]
      (is (vector? report))
      (is (some #(re-find #"All \d+ ignore entries are active" %) report))
      (is (some #(re-find #"No stale ignores found" %) report))))

  (testing "formats report with stale ignores"
    (let [audit-result {:stale {:ignore #{} :ignore-issues [{:file "deleted.md" :line 10 :specimen "test"}]}
                        :active {:ignore #{"word"} :ignore-issues []}}
          report (audit/format-report audit-result)]
      (is (vector? report))
      (is (some #(re-find #"Found \d+ stale" %) report))
      (is (some #(re-find #"deleted\.md" %) report))
      (is (some #(re-find #"proserunner ignore clean" %) report))))

  (testing "includes file paths and line numbers in report"
    (let [audit-result {:stale {:ignore #{} :ignore-issues [{:file "del.md" :line-num 5 :specimen "x"}]}
                        :active {:ignore #{} :ignore-issues []}}
          report (audit/format-report audit-result)
          report-str (apply str report)]
      (is (re-find #"del\.md" report-str))
      (is (re-find #"5" report-str))
      (is (re-find #"x" report-str)))))

(deftest remove-stale-ignores-test
  (testing "removes stale ignores and keeps active ones"
    (let [ignores {:ignore #{"word"}
                   :ignore-issues [{:file "/nonexistent.md" :specimen "stale"}
                                   {:file "test/proserunner/ignore/audit_test.clj" :specimen "active"}]}
          result (audit/remove-stale ignores)]
      (is (= #{"word"} (:ignore result)))
      (is (= 1 (count (:ignore-issues result))))
      (is (= "test/proserunner/ignore/audit_test.clj"
             (:file (first (:ignore-issues result)))))))

  (testing "keeps all ignores when none are stale"
    (let [ignores {:ignore #{"word"}
                   :ignore-issues [{:file "test/proserunner/ignore/audit_test.clj" :specimen "test"}]}
          result (audit/remove-stale ignores)]
      (is (= ignores result))))

  (testing "removes all when all are stale"
    (let [ignores {:ignore #{"word"}
                   :ignore-issues [{:file "/nonexistent.md" :specimen "stale"}]}
          result (audit/remove-stale ignores)]
      (is (= #{"word"} (:ignore result)))
      (is (empty? (:ignore-issues result))))))
