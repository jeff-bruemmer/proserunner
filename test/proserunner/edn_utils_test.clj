(ns proserunner.edn-utils-test
  (:require [clojure.test :as t :refer [deftest is testing use-fixtures]]
            [clojure.java.io :as io]
            [proserunner.edn-utils :as edn-utils]
            [proserunner.result :as result])
  (:import java.io.File))

(def test-dir (atom nil))

(defn setup-test-dir [f]
  (let [tmp-dir (str (System/getProperty "java.io.tmpdir")
                    File/separator
                    "proserunner-edn-test-"
                    (System/currentTimeMillis))]
    (.mkdirs (io/file tmp-dir))
    (reset! test-dir tmp-dir)
    (try
      (f)
      (finally
        (doseq [file (reverse (file-seq (io/file tmp-dir)))]
          (.delete file))))))

(use-fixtures :each setup-test-dir)

(deftest read-edn-file-success
  (testing "read-edn-file returns Success with parsed data"
    (let [filepath (str @test-dir File/separator "test.edn")
          test-data {:foo "bar" :baz [1 2 3]}
          _ (spit filepath (pr-str test-data))
          result (edn-utils/read-edn-file filepath)]

      (is (result/success? result)
          "Should return Success for valid EDN file")
      (is (= test-data (:value result))
          "Should parse EDN data correctly"))))

(deftest read-edn-file-not-found
  (testing "read-edn-file returns Failure for missing file"
    (let [filepath (str @test-dir File/separator "nonexistent.edn")
          result (edn-utils/read-edn-file filepath)]

      (is (result/failure? result)
          "Should return Failure for missing file")
      (is (string? (:error result))
          "Error should be a string")
      (is (contains? (:context result) :filepath)
          "Context should include filepath"))))

(deftest read-edn-file-parse-error
  (testing "read-edn-file returns Failure for invalid EDN"
    (let [filepath (str @test-dir File/separator "invalid.edn")
          _ (spit filepath "{:invalid edn syntax")
          result (edn-utils/read-edn-file filepath)]

      (is (result/failure? result)
          "Should return Failure for invalid EDN")
      (is (string? (:error result))
          "Error should be a string"))))

(deftest read-edn-string-success
  (testing "read-edn-string returns Success with parsed data"
    (let [edn-str "{:foo \"bar\" :baz [1 2 3]}"
          result (edn-utils/read-edn-string edn-str)]

      (is (result/success? result)
          "Should return Success for valid EDN string")
      (is (= {:foo "bar" :baz [1 2 3]} (:value result))
          "Should parse EDN string correctly"))))

(deftest read-edn-string-parse-error
  (testing "read-edn-string returns Failure for invalid EDN"
    (let [edn-str "{:invalid edn"
          result (edn-utils/read-edn-string edn-str)]

      (is (result/failure? result)
          "Should return Failure for invalid EDN string")
      (is (string? (:error result))
          "Error should be a string"))))

(deftest read-edn-string-built-in-tags
  (testing "read-edn-string reads built-in EDN tags like #inst"
    (let [edn-str "#inst \"2024-01-01T00:00:00.000-00:00\""
          result (edn-utils/read-edn-string edn-str)]

      (is (result/success? result)
          "Should handle built-in EDN tags like #inst")
      (is (instance? java.util.Date (:value result))
          "Should parse #inst tag to Date object"))))
