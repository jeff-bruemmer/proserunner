(ns proserunner.path-ignore-test
  (:require [clojure.java.io :as io]
            [clojure.string]
            [clojure.test :as t :refer [deftest is testing]]
            [proserunner.path-ignore :as path-ignore]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [with-temp-dir]])
  (:import java.io.File))

;;; Tests for read-proserunnerignore

(deftest read-proserunnerignore-success
  (testing "read-proserunnerignore returns patterns from valid file"
    (with-temp-dir [temp-dir "path-ignore-test"]
      (let [ignore-file (str temp-dir File/separator ".proserunnerignore")
            patterns ["*.log" "build/**" "# comment" "" "  temp*  "]
            _ (spit ignore-file (clojure.string/join "\n" patterns))
            result (path-ignore/read-proserunnerignore temp-dir)]

        (is (result/success? result)
            "Should return Success for valid ignore file")
        (is (= ["*.log" "build/**" "temp*"] (:value result))
            "Should return trimmed patterns, excluding comments and blanks")))))

(deftest read-proserunnerignore-missing-file
  (testing "read-proserunnerignore returns empty Success when file doesn't exist"
    (with-temp-dir [temp-dir "path-ignore-test"]
      (let [result (path-ignore/read-proserunnerignore temp-dir)]

        (is (result/success? result)
            "Should return Success even when file doesn't exist")
        (is (= [] (:value result))
            "Should return empty vector when file doesn't exist")))))

(deftest read-proserunnerignore-read-error
  (testing "read-proserunnerignore returns Failure when file exists but can't be read"
    (with-temp-dir [temp-dir "path-ignore-test"]
      (let [ignore-file (io/file temp-dir ".proserunnerignore")]
        ;; Create file and make it unreadable
        (spit ignore-file "*.log")
        (.setReadable ignore-file false)

        (let [result (path-ignore/read-proserunnerignore temp-dir)]
          ;; Clean up before assertion (so test cleanup can delete file)
          (.setReadable ignore-file true)

          (is (result/failure? result)
              "Should return Failure when file can't be read")
          (is (string? (:error result))
              "Error should be a string")
          (is (contains? (:context result) :filepath)
              "Context should include filepath")
          (is (contains? (:context result) :operation)
              "Context should include operation"))))))

(deftest read-proserunnerignore-empty-file
  (testing "read-proserunnerignore returns empty Success for file with only comments/blanks"
    (with-temp-dir [temp-dir "path-ignore-test"]
      (let [ignore-file (str temp-dir File/separator ".proserunnerignore")
            _ (spit ignore-file "# just comments\n\n  \n# more comments")
            result (path-ignore/read-proserunnerignore temp-dir)]

        (is (result/success? result))
        (is (= [] (:value result))
            "Should return empty vector when file has only comments/blanks")))))

;;; Tests for glob-to-regex

(deftest glob-to-regex-simple-patterns
  (testing "glob-to-regex converts simple patterns correctly"
    (let [pattern (path-ignore/glob-to-regex "*.txt")]
      (is (re-find pattern "file.txt"))
      (is (re-find pattern "test.txt"))
      (is (not (re-find pattern "file.md")))
      (is (not (re-find pattern "dir/file.txt"))
          "* should not match directory separator"))))

(deftest glob-to-regex-recursive-patterns
  (testing "glob-to-regex handles ** for recursive matching"
    (let [pattern (path-ignore/glob-to-regex "build/**")]
      (is (re-find pattern "build/"))
      (is (re-find pattern "build/file.txt"))
      (is (re-find pattern "build/subdir/file.txt"))
      (is (not (re-find pattern "builds/file.txt"))))))

(deftest glob-to-regex-question-mark
  (testing "glob-to-regex converts ? to match single character"
    (let [pattern (path-ignore/glob-to-regex "file?.txt")]
      (is (re-find pattern "file1.txt"))
      (is (re-find pattern "fileA.txt"))
      (is (not (re-find pattern "file.txt")))
      (is (not (re-find pattern "file12.txt"))))))

(deftest glob-to-regex-escapes-metacharacters
  (testing "glob-to-regex properly escapes regex metacharacters"
    (let [pattern (path-ignore/glob-to-regex "test.file")]
      (is (re-find pattern "test.file"))
      (is (not (re-find pattern "testXfile"))
          "Literal dot should not match any character")))

  (testing "glob-to-regex handles brackets and parens"
    (let [pattern (path-ignore/glob-to-regex "test[1]")]
      (is (re-find pattern "test[1]"))
      (is (not (re-find pattern "test1"))))))

(deftest glob-to-regex-combined-patterns
  (testing "glob-to-regex handles combinations of wildcards"
    (let [pattern (path-ignore/glob-to-regex "src/**/test_*.clj")]
      (is (re-find pattern "src/test_foo.clj"))
      (is (re-find pattern "src/core/test_bar.clj"))
      (is (re-find pattern "src/a/b/c/test_baz.clj"))
      (is (not (re-find pattern "src/test.clj")))
      (is (not (re-find pattern "src/core/test_foo.md"))))))

;;; Tests for should-ignore?

(deftest should-ignore-basic-matching
  (testing "should-ignore? matches files against patterns"
    (let [base-dir "/home/user/project"
          patterns ["*.log" "build/**"]]
      (is (path-ignore/should-ignore?
            "/home/user/project/test.log"
            base-dir
            patterns))
      (is (path-ignore/should-ignore?
            "/home/user/project/build/output.txt"
            base-dir
            patterns))
      (is (not (path-ignore/should-ignore?
                 "/home/user/project/src/core.clj"
                 base-dir
                 patterns))))))

(deftest should-ignore-relative-paths
  (testing "should-ignore? handles paths correctly"
    (let [base-dir "/project"
          patterns ["temp*"]]
      (is (path-ignore/should-ignore?
            "/project/temp.txt"
            base-dir
            patterns))
      (is (path-ignore/should-ignore?
            "/project/temporary.md"
            base-dir
            patterns)))))

(deftest should-ignore-without-base-dir-prefix
  (testing "should-ignore? handles paths that don't start with base-dir"
    (let [base-dir "/project"
          patterns ["*.log"]]
      (is (path-ignore/should-ignore?
            "test.log"
            base-dir
            patterns)))))

(deftest should-ignore-relative-file-paths
  (testing "relative paths, as file-seq yields them for `.` or `docs/`, match patterns relative to base-dir"
    (is (path-ignore/should-ignore? "./drafts/x.md" (.getAbsolutePath (io/file ".")) ["drafts/**"]))
    (is (path-ignore/should-ignore? "docs/drafts/x.md" (.getAbsolutePath (io/file "docs")) ["drafts/**"]))
    (is (path-ignore/should-ignore? "docs/notes.log" (.getAbsolutePath (io/file "docs/")) ["*.log"]))
    (is (not (path-ignore/should-ignore? "docs/b.md" (.getAbsolutePath (io/file "docs")) ["drafts/**"])))))

(deftest should-ignore-empty-patterns
  (testing "should-ignore? returns false/nil for empty patterns"
    (is (not (path-ignore/should-ignore?
               "/any/path.txt"
               "/base"
               [])))))

(deftest should-ignore-subdirectory-patterns
  (testing "should-ignore? matches files in subdirectories correctly"
    (let [base-dir "/project"
          patterns ["docs/**/*.md"]]
      (is (path-ignore/should-ignore?
            "/project/docs/README.md"
            base-dir
            patterns))
      (is (path-ignore/should-ignore?
            "/project/docs/api/guide.md"
            base-dir
            patterns))
      (is (not (path-ignore/should-ignore?
                 "/project/README.md"
                 base-dir
                 patterns)))
      (is (not (path-ignore/should-ignore?
                 "/project/docs/config.json"
                 base-dir
                 patterns))))))
