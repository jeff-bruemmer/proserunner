(ns proserunner.output.format-test
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.string :as string]
            [proserunner.output.format :as format]))

(deftest format-ignored-list-test
  (testing "formats empty ignore list"
    (let [ignores {:ignore #{} :ignore-issues []}
          result (format/ignored-list ignores)]
      (is (vector? result))
      (is (some #(re-find #"No ignored" %) result))))

  (testing "formats simple ignores"
    (let [ignores {:ignore #{"word1" "word2"} :ignore-issues []}
          result (format/ignored-list ignores)]
      (is (some #(re-find #"Simple ignores" %) result))
      (is (some #(re-find #"word1" %) result))))

  (testing "formats contextual ignores"
    (let [ignores {:ignore #{} :ignore-issues [{:file "test.md" :line 10 :specimen "test"}]}
          result (format/ignored-list ignores)]
      (is (some #(re-find #"Contextual ignores" %) result))
      (is (some #(re-find #"test.md" %) result)))))

(deftest format-init-project-test
  (testing "returns vector of strings"
    (let [result (format/init-project {})]
      (is (vector? result))
      (is (every? string? result))
      (is (some #(re-find #"Created project configuration" %) result)))))

(deftest group-results-numbered-test
  (testing "groups results by file"
    ;; This is primarily an integration test - just verify it doesn't throw
    (let [results [{:file "a.md" :line-num 1 :col-num 1 :specimen "test1" :message "msg1"}
                   {:file "a.md" :line-num 2 :col-num 1 :specimen "test2" :message "msg2"}
                   {:file "b.md" :line-num 1 :col-num 1 :specimen "test3" :message "msg3"}]
          output (with-out-str (format/group-numbered results))]
      ;; Verify it produces output
      (is (string? output))
      (is (re-find #"a\.md" output))
      (is (re-find #"b\.md" output))))

  (testing "file headings print without a leading space"
    (let [results [{:file "a.md" :line-num 1 :col-num 1 :specimen "test1" :message "msg1"}]
          output (with-out-str (format/group-numbered results))]
      (is (some #(= "a.md" %) (string/split-lines output))))))

(deftest group-results-alignment-test
  ;; Line and column numbers of differing widths across two files: the columns
  ;; must line up throughout, including across the file boundary.
  (let [results [{:file "a.md" :line-num 7 :col-num 3 :specimen "utilize" :message "Prefer use."}
                 {:file "a.md" :line-num 1024 :col-num 187 :specimen "really" :message "Overused adverb."}
                 {:file "b.md" :line-num 96 :col-num 12 :specimen "at this point in time" :message "Omit this phrase."}]
        output (with-out-str (format/group-numbered results))
        issue-lines (filter #(string/starts-with? % "[") (string/split-lines output))]

    (testing "every issue is printed"
      (is (= 3 (count issue-lines))))

    (testing "no tabs are emitted"
      (is (not (string/includes? output "\t"))))

    (testing "colons align across file groups"
      (is (apply = (map #(string/index-of % ":") issue-lines))))

    (testing "messages align across file groups"
      (is (apply = (map #(string/index-of % "->") issue-lines))))))

(def ^:private sample-issues
  [{:file "a.md" :line-num 3 :col-num 7 :specimen "very" :message "Avoid."
    :name "Hedging" :kind "existence" :line-text "It is very good."}
   {:file "b.md" :line-num 12 :col-num 1 :specimen "the the" :message "Repetition."
    :name "Repetition" :kind "repetition" :line-text "the the end"}])

(deftest plain-numbered-test
  (testing "one issue per line, path:line:col: prefix, numbered across files"
    (let [lines (string/split-lines (with-out-str (format/plain-numbered sample-issues)))]
      (is (= ["a.md:3:7: [1] \"very\" -> Avoid."
              "b.md:12:1: [2] \"the the\" -> Repetition."]
             lines)))))

(deftest table-numbered-columns-test
  (testing "columns come in a fixed order with readable headings"
    (let [header (->> (with-out-str (format/table-numbered sample-issues))
                      string/split-lines
                      (remove string/blank?)
                      first)
          headings (->> (string/split header #"\|")
                        (map string/trim)
                        (remove string/blank?))]
      (is (= ["#" "File" "Line" "Col" "Specimen" "Message" "Name" "Kind"] headings)))))
