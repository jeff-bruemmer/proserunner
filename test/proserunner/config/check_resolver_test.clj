(ns proserunner.config.check-resolver-test
  (:require [clojure.test :refer [deftest is testing use-fixtures]]
            [proserunner.config.check-resolver :as check-resolver]
            [proserunner.test-helpers :refer [delete-recursively temp-dir-path silently]]
            [clojure.java.io :as io])
  (:import java.io.File))

(def test-project-root (atom nil))

(defn setup-test-env [f]
  (let [temp-project (temp-dir-path "proserunner-resolver-test")]
    (reset! test-project-root temp-project)
    (try
      (silently (f))
      (finally
        (delete-recursively (io/file temp-project))))))

(use-fixtures :each setup-test-env)

(deftest resolve-string-reference-to-global-check
  (testing "String references resolve to global check definitions"
    (let [project-root @test-project-root
          global-checks [{:name "default"
                         :directory "default"
                         :files ["cliches" "redundancies"]}]
          check-entries ["default"]
          resolved (check-resolver/resolve-check-entries check-entries global-checks project-root)]
      (is (= 1 (count resolved)))
      (is (= "default" (:name (first resolved))))
      (is (= ["cliches" "redundancies"] (:files (first resolved)))))))

(deftest resolve-map-entry-with-directory
  (testing "Map entries with :directory are normalized"
    (let [project-root @test-project-root
          custom-dir (str project-root File/separator "custom")
          _ (.mkdirs (io/file custom-dir))
          _ (spit (str custom-dir File/separator "style.edn") "{}")
          check-entries [{:directory (str "." File/separator "custom")}]
          resolved (check-resolver/resolve-check-entries check-entries [] project-root)]
      (is (= 1 (count resolved)))
      (is (contains? (first resolved) :directory))
      (is (contains? (first resolved) :files)))))

(deftest resolve-checks-directory-shorthand
  (testing "Special 'checks' directory resolves to .proserunner/checks/"
    (let [project-root @test-project-root
          checks-dir (str project-root File/separator ".proserunner" File/separator "checks")
          _ (.mkdirs (io/file checks-dir))
          _ (spit (str checks-dir File/separator "custom.edn") "{}")
          check-entries [{:directory "checks"}]
          resolved (check-resolver/resolve-check-entries check-entries [] project-root)]
      (is (= 1 (count resolved)))
      (is (= ["custom"] (:files (first resolved))))))

  (testing "The string \"checks\" means the same, not a global directory"
    (let [project-root @test-project-root
          checks-dir (str project-root File/separator ".proserunner" File/separator "checks")
          _ (.mkdirs (io/file checks-dir))
          _ (spit (str checks-dir File/separator "custom.edn") "{}")
          global-checks [{:directory "default" :files ["cliches"]}]
          resolved (check-resolver/resolve-check-entries ["checks"] global-checks project-root)]
      (is (= 1 (count resolved)))
      (is (= checks-dir (:directory (first resolved))))
      (is (= ["custom"] (:files (first resolved)))))))

(deftest auto-discover-edn-files
  (testing "Auto-discovers .edn files when :files not specified"
    (let [project-root @test-project-root
          custom-dir (str project-root File/separator "custom")
          _ (.mkdirs (io/file custom-dir))
          _ (spit (str custom-dir File/separator "style.edn") "{}")
          _ (spit (str custom-dir File/separator "grammar.edn") "{}")
          _ (spit (str custom-dir File/separator "readme.txt") "not a check")
          check-entries [{:directory (str "." File/separator "custom")}]
          resolved (check-resolver/resolve-check-entries check-entries [] project-root)]
      (is (= 1 (count resolved)))
      (is (= ["grammar" "style"] (:files (first resolved)))))))

(deftest resolve-with-explicit-files
  (testing "Respects explicit :files list when provided"
    (let [project-root @test-project-root
          custom-dir (str project-root File/separator "custom")
          _ (.mkdirs (io/file custom-dir))
          _ (spit (str custom-dir File/separator "style.edn") "{}")
          _ (spit (str custom-dir File/separator "grammar.edn") "{}")
          check-entries [{:directory (str "." File/separator "custom")
                         :files ["style"]}]
          resolved (check-resolver/resolve-check-entries check-entries [] project-root)]
      (is (= 1 (count resolved)))
      (is (= ["style"] (:files (first resolved)))))))

(deftest resolve-mixed-entries
  (testing "Resolves mix of string references and map entries"
    (let [project-root @test-project-root
          checks-dir (str project-root File/separator ".proserunner" File/separator "checks")
          _ (.mkdirs (io/file checks-dir))
          _ (spit (str checks-dir File/separator "custom.edn") "{}")
          global-checks [{:directory "default" :files ["cliches"]}]
          check-entries ["default" {:directory "checks"}]
          resolved (check-resolver/resolve-check-entries check-entries global-checks project-root)]
      (is (= 2 (count resolved)))
      (is (= ["cliches"] (:files (first resolved))))
      (is (= ["custom"] (:files (second resolved)))))))

(deftest path-traversal-protection
  (testing "Prevents path traversal outside project root"
    (let [project-root @test-project-root
          check-entries [{:directory (str ".." File/separator "outside")}]
          global-checks []]
      (is (thrown? Exception
                   (check-resolver/resolve-check-entries check-entries global-checks project-root))))))

(deftest resolve-filters-empty-directories
  (testing "Filters out directories with no .edn files"
    (let [project-root @test-project-root
          empty-dir (str project-root File/separator "empty")
          _ (.mkdirs (io/file empty-dir))
          check-entries [{:directory (str "." File/separator "empty")}]
          resolved (check-resolver/resolve-check-entries check-entries [] project-root)]
      (is (empty? resolved)))))

(deftest missing-global-check-directories
  (testing "Identifies missing downloadable global check directories"
    (let [check-entries ["default" "custom" {:directory "checks"}]
          global-checks [{:name "custom" :directory "custom" :files ["style"]}]]
      (is (= #{"default"} (check-resolver/missing-global-check-directories check-entries global-checks)))))

  (testing "Returns empty set when all checks exist"
    (let [check-entries ["default"]
          global-checks [{:name "default" :directory "default" :files ["cliches"]}]]
      (is (empty? (check-resolver/missing-global-check-directories check-entries global-checks)))))

  (testing "Ignores non-downloadable check types"
    (let [check-entries [{:directory "checks"} {:directory "custom"}]
          global-checks []]
      (is (empty? (check-resolver/missing-global-check-directories check-entries global-checks)))))

  (testing "Only identifies 'default' and 'custom' as downloadable"
    (let [check-entries ["default" "custom" "other-string-ref"]
          global-checks []]
      (is (= #{"default" "custom"} (check-resolver/missing-global-check-directories check-entries global-checks))))))
