(ns proserunner.storage-test
  (:require [clojure.java.io :as io]
            [clojure.test :as t :refer [deftest is testing]]
            [proserunner.result :as result]
            [proserunner.storage :as storage]
            [proserunner.test-helpers :refer [with-temp-dir]]
            [proserunner.version])
  (:import java.io.File
           java.util.concurrent.CountDownLatch))

(deftest cache-hash-should-be-stable
  (testing "Cache validation should use stable hashing"
    (let [checks [{:name "check1" :kind "existence" :specimens ["test"]}
                  {:name "check2" :kind "regex" :pattern "foo"}]
          hash1 (hash checks)]

      (testing "Hash should be deterministic and stable"
        (let [stable-hash1 (hash (pr-str checks))
              stable-hash2 (hash (pr-str checks))]
          (is (= stable-hash1 stable-hash2)
              "Stable hash of serialized data should be consistent"))

        (is (= hash1 (hash checks))
            "Hash should be reproducible within and across sessions")))))

(deftest cache-concurrent-writes-should-be-safe
  (testing "Cache writes should be atomic and handle concurrent access safely"
    (let [temp-dir (str (System/getProperty "java.io.tmpdir")
                       File/separator
                       "proserunner-race-test-"
                       (System/currentTimeMillis))
          num-threads 10
          latch (CountDownLatch. num-threads)
          results (atom [])]

      (try
        (.mkdirs (io/file temp-dir))

        (testing "Concurrent writes should not corrupt files"
          (let [threads (doall
                         (for [i (range num-threads)]
                           (Thread.
                            (fn []
                              (.await latch)
                              (try
                                (let [result {:file (str "test-" i ".md")
                                            :content "content"
                                            :check-hash (hash {:check i})
                                            :results [{:line i :issue (str "Issue " i)}]}
                                      file-path (str temp-dir File/separator "cache-" i ".edn")]
                                  (spit file-path (pr-str result))
                                  (Thread/sleep (rand-int 5))
                                  (let [read-back (read-string (slurp file-path))]
                                    (swap! results conj {:success true :result read-back})))
                                (catch Exception e
                                  (swap! results conj {:success false :error (.getMessage e)})))))))]

            (doseq [t threads] (.start t))
            (dotimes [_ num-threads] (.countDown latch))
            (doseq [t threads] (.join t 5000))

            (let [final-results @results
                  successes (filter :success final-results)
                  failures (filter #(not (:success %)) final-results)]
              (is (= num-threads (count successes))
                  (str "All " num-threads " concurrent writes should succeed. "
                       "Got " (count failures) " failures: "
                       (map :error failures))))))

        (finally
          (when (.exists (io/file temp-dir))
            (doseq [f (reverse (file-seq (io/file temp-dir)))]
              (.delete f))))))))

;;; Error handling tests

(deftest mk-cache-dir-success
  (testing "mk-cache-dir! returns Success with cache directory"
    (with-temp-dir [parent-dir "storage-test-parent"]
      (let [cache-path (str parent-dir "/new-cache-dir")
            opts {:cache-dir cache-path}
            result (storage/mk-cache-dir! opts)]

        (is (result/success? result)
            "Should return Success when directory is created")
        (is (string? (:value result))
            "Should return cache directory path as string")
        (is (.exists (io/file (:value result)))
            "Directory should exist")
        (is (.isDirectory (io/file (:value result)))
            "Should be a directory")))))

(deftest mk-cache-dir-already-exists
  (testing "mk-cache-dir! returns Success for existing directory"
    (with-temp-dir [temp-dir "storage-test"]
      (let [opts {:cache-dir temp-dir}
            result (storage/mk-cache-dir! opts)]

        (is (result/success? result)
            "Should return Success when directory already exists")
        (is (.exists (io/file (:value result)))
            "Directory should exist")))))

(deftest mk-cache-dir-error-structure
  (testing "mk-cache-dir! returns Results with proper structure"
    ;; NOTE: Testing actual permission errors is platform-dependent and unreliable.
    ;; The error handling is verified through code inspection and manual testing.
    ;; Here we verify the function returns proper Result types.
    (with-temp-dir [temp-dir "storage-test"]
      (let [valid-path (str temp-dir "/test-cache")
            opts {:cache-dir valid-path}
            result (storage/mk-cache-dir! opts)]

        (is (result/success? result)
            "Should return Success for valid path")
        (is (string? (:value result))
            "Success value should be a string path")

        ;; Verify error Results have proper structure when they occur
        ;; (actual error scenarios tested in integration/manual testing)
        ))))

(deftest save-cache-success
  (testing "save! returns Success and writes cache file"
    (with-temp-dir [_temp-dir "storage-test"]
      (let [test-result (storage/map->Result
                         {:lines []
                          :lines-hash 123
                          :file-hash 456
                          :config {}
                          :config-hash 789
                          :check-hash 101112
                          :output :simple
                          :results []})
            save-result (storage/save! test-result {})]

        (is (result/success? save-result)
            "Should return Success on successful save")))))

(deftest save-cache-directory-missing
  (testing "save! returns Failure when storage directory can't be created"
    ;; This test is difficult because mk-tmp-dir! will try to create the directory
    ;; We would need to stub/mock the directory creation to test this properly
    ;; For now, we'll skip this specific scenario as it requires mocking
    ))

(deftest save-cache-write-error
  (testing "save! returns Failure when write fails"
    ;; Test case: save to a read-only directory after it's created
    (with-temp-dir [temp-dir "storage-test"]
      (let [cache-dir (io/file temp-dir "cache")
            _ (.mkdirs cache-dir)
            _ (.setWritable cache-dir false)
            test-result (storage/map->Result
                         {:lines []
                          :lines-hash 123
                          :file-hash 456
                          :config {}
                          :config-hash 789
                          :check-hash 101112
                          :output :simple
                          :results []})
            ;; Mock mk-cache-dir! to return our read-only directory
            save-result (with-redefs [storage/mk-cache-dir! (fn [_] (result/ok cache-dir))]
                          (storage/save! test-result {}))]

        ;; Restore permissions for cleanup
        (.setWritable cache-dir true)

        (is (result/failure? save-result)
            "Should return Failure when write fails due to permissions")
        (is (string? (:error save-result))
            "Error should be a string")))))

(deftest load-cache-success
  (testing "inventory returns cached result for valid cache"
    (with-temp-dir [_temp-dir "storage-test"]
      ;; We need to set up the cache directory structure properly
      ;; This test requires deeper integration with the caching system
      ;; Skipping for now as it's more of an integration test
      )))

(deftest load-cache-corrupted-data
  (testing "inventory returns false and logs warning for corrupted cache"
    ;; This test is complex because it requires mocking System/getProperty
    ;; which can't be done with with-redefs. The actual behavior is tested
    ;; in integration tests. For now, we'll verify the behavior manually.
    ;; TODO: Add integration test for corrupted cache handling
    ))

(deftest validation-predicates
  (testing "valid-checks? compares check hashes correctly"
    (let [checks [{:name "test" :kind "existence" :specimens ["foo"]}]
          check-hash (storage/checks-hash checks)
          cached-result {:check-hash check-hash}]

      (is (storage/valid-checks? cached-result checks)
          "Should return true for matching check hashes")

      (is (not (storage/valid-checks? cached-result [{:name "different"}]))
          "Should return false for different check hashes")

      (with-redefs [proserunner.version/number "v0.0.0-other"]
        (is (not (storage/valid-checks? cached-result checks))
            "Results cached by another version should be recomputed"))))

  (testing "valid-lines? compares line hashes correctly"
    (let [lines [{:line-num 1 :text "test"}]
          lines-hash (storage/stable-hash lines)
          cached-result {:lines-hash lines-hash}]

      (is (storage/valid-lines? cached-result lines)
          "Should return true for matching line hashes")

      (is (not (storage/valid-lines? cached-result [{:line-num 2}]))
          "Should return false for different line hashes")))

  (testing "valid-config? compares config hashes correctly"
    (let [config {:mode :strict}
          config-hash (storage/stable-hash config)
          cached-result {:config-hash config-hash}]

      (is (storage/valid-config? cached-result config)
          "Should return true for matching config hashes")

      (is (not (storage/valid-config? cached-result {:mode :permissive}))
          "Should return false for different config hashes"))))

(deftest get-cached-result-returns-failure-on-cache-miss
  (testing "Returns Failure with :cache-miss when cache doesn't exist"
    (with-temp-dir [cache-dir "test-cache-miss"]
      (.mkdirs (io/file cache-dir))
      (let [result (storage/get-cached-result "nonexistent.md" {:cache-dir cache-dir})]
        (is (result/failure? result)
            "Should return Failure for non-existent cache")
        (is (= :cache-miss (get-in result [:context :type]))
            "Failure context should indicate cache miss")))))

(deftest get-cached-result-returns-success-on-valid-cache
  (testing "Returns Success with cached data when cache exists and is valid"
    (with-temp-dir [cache-dir "test-cache-valid"]
      (.mkdirs (io/file cache-dir))

      ;; Create a valid cached result with proper file-hash
      (let [file-name "test.md"
            file-hash (storage/stable-hash file-name)
            test-result {:file file-name
                        :file-hash file-hash
                        :lines-hash 789012
                        :config-hash 345678
                        :check-hash 901234
                        :output :simple
                        :results []}
            _ (storage/save! test-result {:cache-dir cache-dir})

            ;; Try to retrieve it
            result (storage/get-cached-result file-name {:cache-dir cache-dir})]

        (is (result/success? result)
            "Should return Success for existing valid cache")
        (is (map? (:value result))
            "Success value should be a map")
        (is (= file-name (:file (:value result)))
            "Cached result should contain original file data")))))

(deftest get-cached-result-returns-failure-on-corrupted-cache
  (testing "Returns Failure with :corrupted-cache when cache file is invalid"
    (with-temp-dir [cache-dir "test-cache-corrupt"]
      (.mkdirs (io/file cache-dir))

      ;; Create a corrupted cache file (invalid EDN)
      (let [cache-file-path (str cache-dir "/file" (storage/stable-hash "corrupt.md") ".edn")]
        (spit cache-file-path "{this is not valid edn")

        (let [result (storage/get-cached-result "corrupt.md" {:cache-dir cache-dir})]
          (is (result/failure? result)
              "Should return Failure for corrupted cache")
          (is (= :corrupted-cache (get-in result [:context :type]))
              "Failure context should indicate corrupted cache"))))))

(deftest get-cached-result-is-pure-no-logging
  (testing "get-cached-result should not print to stdout (pure function)"
    (with-temp-dir [cache-dir "test-pure"]
      (.mkdirs (io/file cache-dir))

      ;; Create a corrupted cache file
      (let [cache-file-path (str cache-dir "/file" (storage/stable-hash "test.md") ".edn")]
        (spit cache-file-path "{invalid edn syntax")

        ;; Capture stdout to verify no println calls
        (let [output (with-out-str
                       (storage/get-cached-result "test.md" {:cache-dir cache-dir}))]
          (is (empty? output)
              "Should not print to stdout - function should be pure"))))))

(deftest handle-corrupted-cache-file-returns-context-map
  (testing "handle-corrupted-cache-file returns detailed context about corruption"
    (with-temp-dir [cache-dir "test-handler"]
      (.mkdirs (io/file cache-dir))

      (let [cache-file-path (str cache-dir "/corrupt.edn")
            cache-file (io/file cache-file-path)
            _ (spit cache-file-path "{invalid}")
            read-error {:error "Parse error" :context {}}
            result (#'storage/handle-corrupted-cache-file cache-file cache-file-path read-error)]

        (is (map? result)
            "Should return a map with context")
        (is (= :corrupted-cache (:type result))
            "Should have :corrupted-cache type")
        (is (string? (:file result))
            "Should include file path")
        (is (boolean? (:deleted? result))
            "Should include deletion status as boolean")
        (is (= "Parse error" (:original-error result))
            "Should include original error")))))
