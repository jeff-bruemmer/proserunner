(ns proserunner.file-utils-test
  (:require [clojure.test :as t :refer [deftest is testing use-fixtures]]
            [clojure.java.io :as io]
            [clojure.edn :as edn]
            [proserunner.file-utils :as file-utils]
            [proserunner.result :as result])
  (:import java.io.File))

(def test-dir (atom nil))

(defn setup-test-dir [f]
  (let [tmp-dir (str (System/getProperty "java.io.tmpdir")
                    File/separator
                    "proserunner-file-utils-test-"
                    (System/currentTimeMillis))]
    (.mkdirs (io/file tmp-dir))
    (reset! test-dir tmp-dir)
    (try
      (f)
      (finally
        (doseq [file (reverse (file-seq (io/file tmp-dir)))]
          (.delete file))))))

(use-fixtures :each setup-test-dir)

(deftest join-path-basic
  (testing "join-path combines path components"
    (let [result (file-utils/join-path "home" "user" "file.txt")]
      (is (string? result))
      (is (.contains result "home"))
      (is (.contains result "user"))
      (is (.contains result "file.txt")))))

(deftest ensure-parent-dir-creates-directory
  (testing "ensure-parent-dir creates parent directories if they don't exist"
    (let [nested-path (file-utils/join-path @test-dir "a" "b" "c" "file.txt")
          parent-dir (file-utils/join-path @test-dir "a" "b" "c")]

      (is (not (.exists (io/file parent-dir)))
          "Parent directory should not exist initially")

      (file-utils/ensure-parent-dir nested-path)

      (is (.exists (io/file parent-dir))
          "Parent directory should be created")
      (is (.isDirectory (io/file parent-dir))
          "Parent should be a directory"))))

(deftest ensure-parent-dir-idempotent
  (testing "ensure-parent-dir is idempotent"
    (let [filepath (file-utils/join-path @test-dir "x" "file.txt")]

      (file-utils/ensure-parent-dir filepath)
      (file-utils/ensure-parent-dir filepath)

      (is (.exists (io/file @test-dir "x"))
          "Should work when called multiple times"))))

(deftest write-edn-file-success
  (testing "write-edn-file writes EDN data to file"
    (let [filepath (file-utils/join-path @test-dir "test.edn")
          test-data {:foo "bar" :baz [1 2 3]}
          result (file-utils/write-edn-file filepath test-data)]

      (is (result/success? result)
          "Should return Success")
      (is (.exists (io/file filepath))
          "File should be created")

      (let [content (slurp filepath)
            parsed (edn/read-string content)]
        (is (= test-data parsed)
            "Written data should match input")))))

(deftest write-edn-file-creates-parents
  (testing "write-edn-file creates parent directories"
    (let [nested-path (file-utils/join-path @test-dir "nested" "dir" "config.edn")
          test-data {:test true}
          result (file-utils/write-edn-file nested-path test-data)]

      (is (result/success? result)
          "Should return Success even with nested path")
      (is (.exists (io/file nested-path))
          "Nested file should be created"))))

(deftest write-edn-file-overwrites
  (testing "write-edn-file overwrites existing file"
    (let [filepath (file-utils/join-path @test-dir "overwrite.edn")
          original {:version 1}
          updated {:version 2}]

      (file-utils/write-edn-file filepath original)
      (file-utils/write-edn-file filepath updated)

      (let [parsed (clojure.edn/read-string (slurp filepath))]
        (is (= updated parsed)
            "File should be overwritten with new data")))))

(deftest mkdirs-if-missing-creates-directory
  (testing "mkdirs-if-missing creates directory structure"
    (let [dirpath (file-utils/join-path @test-dir "new" "nested" "dir")]

      (is (not (.exists (io/file dirpath)))
          "Directory should not exist initially")

      (file-utils/mkdirs-if-missing dirpath)

      (is (.exists (io/file dirpath))
          "Directory should be created")
      (is (.isDirectory (io/file dirpath))
          "Path should be a directory"))))

(deftest mkdirs-if-missing-idempotent
  (testing "mkdirs-if-missing is idempotent"
    (let [dirpath (file-utils/join-path @test-dir "idempotent")]

      (file-utils/mkdirs-if-missing dirpath)
      (file-utils/mkdirs-if-missing dirpath)
      (file-utils/mkdirs-if-missing dirpath)

      (is (.exists (io/file dirpath))
          "Should work when called multiple times")
      (is (.isDirectory (io/file dirpath))
          "Should remain a directory"))))

;;; Atomic operation cleanup tests

(deftest atomic-spit-success
  (testing "atomic-spit successfully writes file"
    (let [filepath (file-utils/join-path @test-dir "atomic-test.txt")
          content "test content"]

      (file-utils/atomic-spit filepath content)

      (is (.exists (io/file filepath))
          "File should be created")
      (is (= content (slurp filepath))
          "Content should match"))))

(deftest atomic-spit-temp-file-cleanup
  (testing "atomic-spit cleans up temp file on write failure"
    (let [parent-dir (io/file @test-dir "readonly-parent")
          _ (.mkdirs parent-dir)
          _ (.setWritable parent-dir false)  ; Make directory read-only
          filepath (file-utils/join-path (.getPath parent-dir) "file.txt")
          temp-files-before (filter #(.startsWith (.getName %) ".proserunner-")
                                   (.listFiles (io/file @test-dir)))]

      ;; Attempt write (should fail due to permissions)
      (try
        (file-utils/atomic-spit filepath "content")
        (catch Exception _))

      ;; Restore permissions for cleanup
      (.setWritable parent-dir true)

      ;; Check for leaked temp files in parent of parent (where temp files are created)
      (let [temp-files-after (filter #(.startsWith (.getName %) ".proserunner-")
                                    (.listFiles (io/file @test-dir)))]
        (is (= (count temp-files-before) (count temp-files-after))
            "Should not leak temp files even when write fails")))))

(deftest atomic-spit-overwrites-existing
  (testing "atomic-spit atomically overwrites existing file"
    (let [filepath (file-utils/join-path @test-dir "overwrite-atomic.txt")]

      (spit filepath "original")
      (is (= "original" (slurp filepath)))

      (file-utils/atomic-spit filepath "updated")
      (is (= "updated" (slurp filepath))
          "Should atomically replace file content"))))

(deftest atomic-spit-concurrent-writes
  (testing "atomic-spit handles concurrent writes safely"
    (let [filepath (file-utils/join-path @test-dir "concurrent.txt")
          num-threads 5
          latch (java.util.concurrent.CountDownLatch. num-threads)
          results (atom [])]

      (doseq [i (range num-threads)]
        (.start
         (Thread.
          (fn []
            (.await latch)
            (try
              (file-utils/atomic-spit filepath (str "thread-" i))
              (swap! results conj {:success true :thread i})
              (catch Exception e
                (swap! results conj {:success false :thread i :error (.getMessage e)})))))))

      ;; Start all threads simultaneously
      (dotimes [_ num-threads] (.countDown latch))
      (Thread/sleep 200)  ; Give threads time to complete

      ;; Verify file exists and no exceptions occurred
      (is (.exists (io/file filepath))
          "File should exist after concurrent writes")
      (is (every? :success @results)
          "All concurrent writes should succeed without exceptions"))))

;;; Path normalization tests

(deftest normalize-path-relative-stays-relative
  (testing "normalize-path keeps relative paths relative"
    (let [relative-path "docs/file.md"
          result (file-utils/normalize-path relative-path)]
      (is (= "docs/file.md" result)
          "Relative path should remain unchanged"))))

(deftest normalize-path-absolute-under-cwd
  (testing "normalize-path converts absolute paths under cwd to relative"
    (let [cwd (System/getProperty "user.dir")
          absolute-path (file-utils/join-path cwd "docs" "file.md")
          result (file-utils/normalize-path absolute-path)]
      (is (= (file-utils/join-path "docs" "file.md") result)
          "Absolute path under cwd should become relative"))))

(deftest normalize-path-dot-slash-prefix
  (testing "normalize-path handles ./ prefix in paths"
    (let [dot-path "./docs/file.md"
          result (file-utils/normalize-path dot-path)]
      (is (= (file-utils/join-path "docs" "file.md") result)
          "./ prefix should be normalized"))))

(deftest normalize-path-outside-cwd
  (testing "normalize-path keeps absolute paths outside cwd as absolute"
    (let [outside-path "/tmp/somewhere/file.md"
          result (file-utils/normalize-path outside-path)]
      (is (= outside-path result)
          "Paths outside cwd should remain absolute"))))

(deftest delete-tree-test
  (let [base (str (System/getProperty "java.io.tmpdir") File/separator
                  "proserunner-delete-tree-" (System/nanoTime))
        tree (io/file base "tree")
        outside (io/file base "outside")]
    (try
      (io/make-parents (io/file tree "a/b/file.txt"))
      (spit (io/file tree "a/b/file.txt") "x")
      (io/make-parents (io/file outside "keep.txt"))
      (spit (io/file outside "keep.txt") "keep")
      (java.nio.file.Files/createSymbolicLink
       (.toPath (io/file tree "link"))
       (.toPath outside)
       (make-array java.nio.file.attribute.FileAttribute 0))

      (file-utils/delete-tree! tree)

      (testing "deletes the directory and everything in it"
        (is (not (.exists tree))))
      (testing "doesn't follow symlinks out of the tree"
        (is (= "keep" (slurp (io/file outside "keep.txt")))))
      (testing "missing paths are fine"
        (is (some? (file-utils/delete-tree! tree))))
      (finally
        (file-utils/delete-tree! base)))))
