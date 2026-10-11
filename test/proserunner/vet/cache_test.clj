(ns proserunner.vet.cache-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.vet.cache :as cache]
            [proserunner.storage :as storage]
            [proserunner.config.types :as types]))

(deftest compute-changed-produces-valid-cache
  (testing "compute-changed creates results that pass validation checks"
    (let [lines [{:text "Hello world" :line-num 1}
                 {:text "Goodbye" :line-num 2}]
          cfg (types/map->Config {:checks [] :ignore "ignore"})
          checks []
          file "test.txt"

          ;; Create initial cached result
          cached-result (storage/map->Result
                         {:lines lines
                          :lines-hash (storage/stable-hash lines)
                          :file-hash (storage/stable-hash file)
                          :config cfg
                          :config-hash (storage/stable-hash cfg)
                          :check-hash (storage/stable-hash checks)
                          :results []})

          ;; Add a new line
          new-lines (conj lines {:text "New line" :line-num 3})

          input {:file file
                 :lines new-lines
                 :config cfg
                 :checks checks
                 :cached-result cached-result
                 :output :json
                 :parallel-lines 1}

          ;; Mock process function that returns empty results
          mock-process (fn [_checks _lines _parallel] [])

          ;; Compute changed creates a new result
          result (cache/compute-changed input mock-process)]

      ;; The critical test: can we validate this result?
      ;; This test would have caught the bug where compute-changed used unstable hash
      (is (storage/valid-lines? result new-lines)
          "Result should validate against new lines")
      (is (storage/valid-config? result cfg)
          "Result should validate against config")
      (is (storage/valid-checks? result checks)
          "Result should validate against checks")

      ;; Verify the result has the expected structure
      (is (= new-lines (:lines result))
          "Result should contain new lines")
      (is (= cfg (:config result))
          "Result should contain config"))))

(deftest compute-changed-processes-only-new-lines
  (testing "compute-changed only processes lines that changed"
    (let [line1 {:text "Unchanged line" :line-num 1}
          line2 {:text "Another unchanged" :line-num 2}
          original-lines [line1 line2]

          cfg (types/map->Config {:checks [] :ignore "ignore"})
          checks []
          file "test.txt"

          ;; Original cached result with some issues
          cached-result (storage/map->Result
                         {:lines original-lines
                          :lines-hash (storage/stable-hash original-lines)
                          :file-hash (storage/stable-hash file)
                          :config cfg
                          :config-hash (storage/stable-hash cfg)
                          :check-hash (storage/stable-hash checks)
                          :results [{:text "Unchanged line"
                                     :line-num 1
                                     :issue "test issue"}]})

          ;; Add a new line, keep original lines
          new-line {:text "New line" :line-num 3}
          new-lines [line1 line2 new-line]

          input {:file file
                 :lines new-lines
                 :config cfg
                 :checks checks
                 :cached-result cached-result
                 :output :json
                 :parallel-lines 1}

          ;; Track which lines were processed
          processed-lines (atom [])
          mock-process (fn [_checks lines _parallel]
                         (reset! processed-lines lines)
                         [])

          result (cache/compute-changed input mock-process)]

      ;; Only the new line should have been processed
      (is (= [new-line] @processed-lines)
          "Should only process the new line, not unchanged lines")

      ;; Old results should be preserved
      (is (some #(= "test issue" (:issue %)) (:results result))
          "Should preserve cached results for unchanged lines"))))

(deftest compute-changed-handles-duplicate-lines
  (testing "compute-changed preserves results for duplicate line text

    This test ensures that when multiple lines have identical text (e.g., lines 3
    and 6 both contain 'Same text'), all issues are correctly preserved and
    displayed.

    The cache excludes duplicate lines to avoid incorrect line number mapping.
    When a file is edited and the cache is used, duplicate lines are always
    reprocessed rather than retrieved from cache. This test verifies that all
    issues for duplicate lines appear in the final results."
    (let [line1 {:text "Unique line" :line-num 1}
          line2 {:text "Different line" :line-num 2}
          line3 {:text "Same text" :line-num 3}
          line4 {:text "Another unique" :line-num 4}
          line5 {:text "More unique" :line-num 5}
          line6 {:text "Same text" :line-num 6}
          original-lines [line1 line2 line3 line4 line5 line6]

          cfg (types/map->Config {:checks [] :ignore "ignore"})
          checks []
          file "test.txt"

          ;; Cached result with issues on BOTH duplicate lines
          cached-result (storage/map->Result
                         {:lines original-lines
                          :lines-hash (storage/stable-hash original-lines)
                          :file-hash (storage/stable-hash file)
                          :config cfg
                          :config-hash (storage/stable-hash cfg)
                          :check-hash (storage/stable-hash checks)
                          :results [{:text "Same text"
                                     :line-num 3
                                     :issue "issue on line 3"}
                                    {:text "Same text"
                                     :line-num 6
                                     :issue "issue on line 6"}]})

          ;; Add a new line to trigger cache path (but keep duplicate lines)
          new-line {:text "Brand new line" :line-num 7}
          new-lines (conj original-lines new-line)

          input {:file file
                 :lines new-lines
                 :config cfg
                 :checks checks
                 :cached-result cached-result
                 :output :json
                 :parallel-lines 1}

          ;; Mock returns results for duplicate lines (they're reprocessed, not cached)
          mock-process (fn [_checks _lines _parallel]
                         ;; Return issues for duplicate line text
                         (filter #(= "Same text" (:text %))
                                 [{:text "Same text"
                                   :line-num 3
                                   :issue "issue on line 3"}
                                  {:text "Same text"
                                   :line-num 6
                                   :issue "issue on line 6"}]))

          result (cache/compute-changed input mock-process)]

      ;; Both duplicate line results should be present (from reprocessing, not cache)
      (is (some #(and (= "Same text" (:text %))
                      (= 3 (:line-num %))
                      (= "issue on line 3" (:issue %)))
                (:results result))
          "Should have result for first duplicate line (line 3)")

      (is (some #(and (= "Same text" (:text %))
                      (= 6 (:line-num %))
                      (= "issue on line 6" (:issue %)))
                (:results result))
          "Should have result for second duplicate line (line 6)"))))
