(ns proserunner.cache-dir-integration-test
  "Integration tests for --cache-dir CLI option"
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.java.io :as io]
            [proserunner.test-helpers :refer [temp-dir-path delete-recursively create-temp-dir! silently]]
            [proserunner.core :as core]
            [proserunner.vet.processor :as processor]))

(deftest cache-dir-option-creates-cache-in-custom-location
  (testing "Cache files created in directory specified by --cache-dir"
    (let [custom-cache (temp-dir-path "custom-cache")
          test-file-dir (create-temp-dir! "test-md-files")
          test-file (io/file test-file-dir "sample.md")]

      ;; Create a simple test markdown file
      (spit test-file "This is obviously a test file.\n")

      (try
        ;; Create custom cache directory
        (.mkdirs (io/file custom-cache))

        ;; Run proserunner with custom cache dir
        (silently
          (core/reception ["-d" custom-cache "-f" (.getPath test-file)]))

        ;; Verify cache file exists in custom location
        (let [cache-files (seq (.listFiles (io/file custom-cache)))]
          (is (some? cache-files)
              "Cache directory should contain at least one file")
          (is (> (count cache-files) 0)
              "Cache directory should not be empty")
          (is (every? #(re-find #"^file.*\.edn$" (.getName %)) cache-files)
              "Cache files should match pattern file*.edn"))

        (finally
          (delete-recursively (io/file custom-cache))
          (delete-recursively test-file-dir))))))

(deftest cache-dir-option-isolates-different-cache-locations
  (testing "Different cache-dir values create separate caches"
    (let [cache-a (temp-dir-path "cache-a")
          cache-b (temp-dir-path "cache-b")
          test-file-dir (create-temp-dir! "test-md-files-2")
          test-file (io/file test-file-dir "sample.md")]

      (spit test-file "This is clearly a test.\n")

      (try
        (.mkdirs (io/file cache-a))
        (.mkdirs (io/file cache-b))

        ;; Run with cache-a
        (silently
          (core/reception ["-d" cache-a "-f" (.getPath test-file)]))

        ;; Run with cache-b
        (silently
          (core/reception ["-d" cache-b "-f" (.getPath test-file)]))

        ;; Both should have cache files
        (is (> (count (.listFiles (io/file cache-a))) 0)
            "Cache A should have files")
        (is (> (count (.listFiles (io/file cache-b))) 0)
            "Cache B should have files")

        (finally
          (delete-recursively (io/file cache-a))
          (delete-recursively (io/file cache-b))
          (delete-recursively test-file-dir))))))

(deftest cache-dir-option-is-read-back
  (testing "A second run with the same --cache-dir reuses the cached result"
    (let [custom-cache (temp-dir-path "cache-read-back")
          test-file-dir (create-temp-dir! "test-md-files-3")
          test-file (io/file test-file-dir "sample.md")
          runs (atom 0)
          process processor/process]
      (spit test-file "This is obviously a test file.\n")
      (try
        (with-redefs [processor/process (fn [& args] (swap! runs inc) (apply process args))]
          (dotimes [_ 2]
            (silently
              (core/reception ["--cache-dir" custom-cache (.getPath test-file)]))))
        (is (= 1 @runs) "Only the first run should check lines")
        (finally
          (delete-recursively (io/file custom-cache))
          (delete-recursively test-file-dir))))))
