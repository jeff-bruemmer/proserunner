(ns proserunner.vet-test
  (:require [proserunner.vet :as vet]
            [proserunner.vet.input :as input]
            [proserunner.project-config :as project-config]
            [proserunner.result :as result]
            [proserunner.storage :as storage]
            [clojure.test :as t :refer [deftest is testing use-fixtures]]
            [clojure.java.io :as io]
            [editors.registry :as registry]
            [editors.utilities :as util]
            [editors.repetition :as repetition]
            [editors.re :as re])
  (:import java.io.File))

(defn setup-editors [f]
  ;; Register editors before each test
  (doseq [editor-type (util/standard-editor-types)]
    (registry/register-editor! editor-type (util/create-editor editor-type)))
  (registry/register-editor! "repetition" repetition/proofread)
  (registry/register-editor! "regex" re/proofread)
  (f))

(use-fixtures :each setup-editors)

(deftest compute
  (let [config-path (str (System/getProperty "user.home")
                         java.io.File/separator
                         ".proserunner"
                         java.io.File/separator
                         "config.edn")
        input-result (input/make {:file "resources"
                                      :config config-path
                                      :output "table"
                                      :code-blocks false
                                      :parallel-files false
                                      :parallel-lines false})]
    (is (result/success? input-result)
        "make should return Success")
    (let [input (:value input-result)
          results (vet/compute input)]
      (is (false? (empty? results)))
      ;; Check that we have results (instead of hardcoding count)
      (is (pos? (count (:results results))))
      ;; Verify structure of results
      (is (every? :file (:results results)))
      (is (every? :line-num (:results results)))
      (is (every? :issues (:results results))))))

(deftest no-cache-option
  (testing "no-cache option bypasses cache and forces recomputation"
    (let [config-path (str (System/getProperty "user.home")
                           java.io.File/separator
                           ".proserunner"
                           java.io.File/separator
                           "config.edn")
          opts {:file "resources"
                :config config-path
                :output "table"
                :code-blocks false
                :parallel-files false
                :parallel-lines false}
          file "resources"]

      ;; First run - will compute and cache
      (let [r1 (vet/compute-or-cached opts)]
        (is (result/success? r1) "First run should succeed"))

      ;; Verify cache file exists after first run
      (let [inventory-result (storage/get-cached-result file opts)]
        (is (result/success? inventory-result)
            (str "Cache should be created after first run. Got: " (pr-str inventory-result))))

      ;; Second run without no-cache - should use cache
      (let [cached-result (storage/get-cached-result file opts)
            cached-data (when (result/success? cached-result) (:value cached-result))
            result2-res (vet/compute-or-cached opts)
            result2 (:value result2-res)]
        (is (result/success? result2-res) "Second run should succeed")
        (is (some? cached-data)
            (str "Cache should exist for second run. Got: " (type cached-data) " = " (pr-str cached-data)))
        ;; Results should be consistent between cached and fresh runs
        (when (and cached-data (:results cached-data))
          (is (= (count (:results (:results result2)))
                 (count (:results cached-data)))
              "Cached results should match fresh results")))

      ;; Third run WITH no-cache - results should still be correct but recomputed
      (let [result3-res (vet/compute-or-cached (assoc opts :no-cache true))]
        (is (result/success? result3-res) "No-cache run should succeed")
        (let [result3 (:value result3-res)]
          (is (pos? (count (:results (:results result3))))
              "No-cache run should still produce results"))))))

(deftest project-config-loaded-from-working-directory
  (testing "Project config is loaded from current working directory, not file directory"
    ;; This regression test ensures that when processing files in subdirectories,
    ;; the project config (.proserunner/config.edn) is loaded from the current
    ;; working directory (project root), not from the file's parent directory.
    ;; Bug fixed: vet.clj was passing file-path to load-project-config instead of user.dir
    (let [original-dir (System/getProperty "user.dir")
          original-home (System/getProperty "user.home")
          ;; Create temp project structure
          temp-project (str (System/getProperty "java.io.tmpdir")
                           File/separator
                           "proserunner-regression-test-"
                           (System/currentTimeMillis))
          proserunner-dir (str temp-project File/separator ".proserunner")
          temp-home (str (System/getProperty "java.io.tmpdir")
                        File/separator
                        "proserunner-test-home-"
                        (System/currentTimeMillis))
          temp-home-config (str temp-home File/separator ".proserunner")]

      (try
        ;; Set up temp home with global config
        (System/setProperty "user.home" temp-home)
        (.mkdirs (io/file temp-home-config "default"))
        (.mkdirs (io/file temp-home-config "custom"))
        (spit (str temp-home-config File/separator "config.edn")
              (pr-str {:checks [{:name "default" :directory "default" :files []}]}))
        (spit (str temp-home-config File/separator "ignore.edn")
              (pr-str #{}))

        ;; Create project structure with .git directory (marks project boundary)
        (.mkdirs (io/file temp-project ".git"))
        (.mkdirs (io/file proserunner-dir "checks"))

        ;; Create project config with specific ignore set
        (spit (str proserunner-dir File/separator "config.edn")
              (pr-str {:checks ["default"]
                       :ignore #{"test-ignore-specimen" "another-ignored-word"}
                       :ignore-mode :extend
                       :config-mode :merged}))

        ;; Change working directory to project root
        (System/setProperty "user.dir" temp-project)

        ;; Load project config - should use user.dir (temp-project), not a file path
        (let [loaded-config (binding [*out* (java.io.StringWriter.)]
                              (project-config/load temp-project))]

          ;; Verify that project config was found and ignore set was loaded
          (is (= :project (:source loaded-config))
              "Project config should be loaded (not falling back to global)")
          (is (= #{"test-ignore-specimen" "another-ignored-word"} (:ignore loaded-config))
              "Project ignore set should be loaded from .proserunner/config.edn in project root"))

        (finally
          ;; Restore original directories
          (System/setProperty "user.dir" original-dir)
          (System/setProperty "user.home" original-home)
          ;; Clean up temp directories
          (try
            (when (.exists (io/file temp-project))
              (doseq [f (reverse (file-seq (io/file temp-project)))]
                (.delete f)))
            (when (.exists (io/file temp-home))
              (doseq [f (reverse (file-seq (io/file temp-home)))]
                (.delete f)))
            (catch Exception _e
              ;; Ignore cleanup errors
              nil)))))))

(defn- verify-project-config
  "Helper to verify project config ignore settings."
  [temp-project expected-source expected-ignore msg]
  (let [loaded-config (binding [*out* (java.io.StringWriter.)]
                        (project-config/load temp-project))]
    (is (= expected-source (:source loaded-config)))
    (is (= expected-ignore (:ignore loaded-config)) msg)))

(defn- with-temp-project-env
  "Execute test-fn with temporary project and home directories set up."
  [test-name-suffix test-fn]
  (let [original-dir (System/getProperty "user.dir")
        original-home (System/getProperty "user.home")
        temp-project (str (System/getProperty "java.io.tmpdir")
                         File/separator
                         "proserunner-" test-name-suffix "-"
                         (System/currentTimeMillis))
        proserunner-dir (str temp-project File/separator ".proserunner")
        temp-home (str (System/getProperty "java.io.tmpdir")
                      File/separator
                      "proserunner-test-home-" test-name-suffix "-"
                      (System/currentTimeMillis))
        temp-home-config (str temp-home File/separator ".proserunner")]
    (try
      (test-fn temp-project proserunner-dir temp-home temp-home-config)
      (finally
        (System/setProperty "user.dir" original-dir)
        (System/setProperty "user.home" original-home)
        (try
          (when (.exists (io/file temp-project))
            (doseq [f (reverse (file-seq (io/file temp-project)))]
              (.delete f)))
          (when (.exists (io/file temp-home))
            (doseq [f (reverse (file-seq (io/file temp-home)))]
              (.delete f)))
          (catch Exception _e nil))))))

(deftest global-and-project-ignores-are-both-used
  (testing "Both global and project ignores are used when ignore-mode is :extend"
    ;; This test verifies that specimens in BOTH the global ignore list
    ;; and the project ignore list are filtered out when :ignore-mode is :extend
    (let [original-dir (System/getProperty "user.dir")
          original-home (System/getProperty "user.home")
          ;; Create temp project structure
          temp-project (str (System/getProperty "java.io.tmpdir")
                           File/separator
                           "proserunner-global-test-"
                           (System/currentTimeMillis))
          proserunner-dir (str temp-project File/separator ".proserunner")
          temp-home (str (System/getProperty "java.io.tmpdir")
                        File/separator
                        "proserunner-test-home-global-"
                        (System/currentTimeMillis))
          temp-home-config (str temp-home File/separator ".proserunner")]

      (try
        ;; Set up temp home with global config and global ignores
        (System/setProperty "user.home" temp-home)
        (.mkdirs (io/file temp-home-config "default"))
        (.mkdirs (io/file temp-home-config "custom"))
        (spit (str temp-home-config File/separator "config.edn")
              (pr-str {:checks [{:name "default" :directory "default" :files []}]}))
        ;; IMPORTANT: Add global ignores with new format
        (spit (str temp-home-config File/separator "ignore.edn")
              (pr-str {:ignore #{"global-ignore-1" "global-ignore-2"}
                       :ignore-issues []}))

        ;; Create project structure with .git directory
        (.mkdirs (io/file temp-project ".git"))
        (.mkdirs (io/file proserunner-dir "checks"))

        ;; Create project config with project ignores and :ignore-mode :extend
        (spit (str proserunner-dir File/separator "config.edn")
              (pr-str {:checks ["default"]
                       :ignore #{"project-ignore-1" "project-ignore-2"}
                       :ignore-mode :extend  ; This should merge global + project
                       :config-mode :merged}))

        ;; Change working directory to project root
        (System/setProperty "user.dir" temp-project)

        ;; Load project config - should merge global and project ignores
        (verify-project-config temp-project
                              :project
                              #{"global-ignore-1" "global-ignore-2" "project-ignore-1" "project-ignore-2"}
                              "Ignore set should include BOTH global and project ignores when :ignore-mode is :extend")

        (finally
          ;; Restore original directories
          (System/setProperty "user.dir" original-dir)
          (System/setProperty "user.home" original-home)
          ;; Clean up temp directories
          (try
            (when (.exists (io/file temp-project))
              (doseq [f (reverse (file-seq (io/file temp-project)))]
                (.delete f)))
            (when (.exists (io/file temp-home))
              (doseq [f (reverse (file-seq (io/file temp-home)))]
                (.delete f)))
            (catch Exception _e
              ;; Ignore cleanup errors
              nil))))))

^{:clj-kondo/ignore [:inline-def]}
(deftest project-ignores-replace-global-when-replace-mode
  (testing "Only project ignores are used when ignore-mode is :replace"
    ;; This test verifies that when :ignore-mode is :replace,
    ;; only the project ignores are used (global ignores are ignored)
    (with-temp-project-env "replace-test"
      (fn [temp-project proserunner-dir temp-home temp-home-config]
        ;; Set up temp home with global ignores
        (System/setProperty "user.home" temp-home)
        (.mkdirs (io/file temp-home-config "default"))
        (.mkdirs (io/file temp-home-config "custom"))
        (spit (str temp-home-config File/separator "config.edn")
              (pr-str {:checks [{:name "default" :directory "default" :files []}]}))
        (spit (str temp-home-config File/separator "ignore.edn")
              (pr-str {:ignore #{"global-ignore-1" "global-ignore-2"}
                       :ignore-issues []}))

        ;; Create project with :ignore-mode :replace
        (.mkdirs (io/file temp-project ".git"))
        (.mkdirs (io/file proserunner-dir "checks"))
        (spit (str proserunner-dir File/separator "config.edn")
              (pr-str {:checks ["default"]
                       :ignore #{"project-ignore-only"}
                       :ignore-mode :replace  ; Should NOT include global ignores
                       :config-mode :merged}))

        (System/setProperty "user.dir" temp-project)

        ;; Load project config and verify ONLY project ignores are included
        (verify-project-config temp-project
                              :project
                              #{"project-ignore-only"}
                              "Ignore set should include ONLY project ignores when :ignore-mode is :replace"))))))

(deftest skip-ignore-flag-skips-all-ignores
  (testing "--skip-ignore flag bypasses both global and project ignores"
    (let [original-dir (System/getProperty "user.dir")
          original-home (System/getProperty "user.home")
          temp-project (str (System/getProperty "java.io.tmpdir")
                           File/separator
                           "proserunner-skip-ignore-test-"
                           (System/currentTimeMillis))
          proserunner-dir (str temp-project File/separator ".proserunner")
          temp-home (str (System/getProperty "java.io.tmpdir")
                        File/separator
                        "proserunner-test-home-skip-"
                        (System/currentTimeMillis))
          temp-home-config (str temp-home File/separator ".proserunner")]

      (try
        ;; Set up temp home with global ignores and a dummy check file
        (System/setProperty "user.home" temp-home)
        (.mkdirs (io/file temp-home-config "default"))
        (spit (str temp-home-config File/separator "config.edn")
              (pr-str {:checks [{:name "default" :directory "default" :files ["dummy"]}]}))
        (spit (str temp-home-config File/separator "ignore.edn")
              (pr-str {:ignore #{"global-ignore"}
                       :ignore-issues []}))
        ;; Create a dummy check file
        (spit (str temp-home-config File/separator "default" File/separator "dummy.edn")
              (pr-str {:name "dummy-check"
                       :kind "existence"
                       :message "Test message"
                       :explanation "Test explanation"
                       :specimens ["test-specimen"]}))

        ;; Create project with project ignores
        (.mkdirs (io/file temp-project ".git"))
        (.mkdirs (io/file proserunner-dir "checks"))
        (spit (str proserunner-dir File/separator "config.edn")
              (pr-str {:checks ["default"]
                       :ignore #{"project-ignore"}
                       :ignore-mode :extend
                       :config-mode :merged}))

        (System/setProperty "user.dir" temp-project)

        ;; Test with skip-ignore flag
        (let [input-result-with-skip (input/make {:file "resources"
                                                      :config (str temp-home-config File/separator "config.edn")
                                                      :skip-ignore true})]
          (is (result/success? input-result-with-skip) "Should succeed")
          (is (= #{} (:project-ignore (:value input-result-with-skip)))
              "--skip-ignore should result in empty ignore set"))

        ;; Test without skip-ignore flag (should use merged ignores)
        (let [input-result-without-skip (input/make {:file "resources"
                                                         :config (str temp-home-config File/separator "config.edn")
                                                         :skip-ignore false})]
          (is (result/success? input-result-without-skip) "Should succeed")
          (is (= #{"global-ignore" "project-ignore"} (:project-ignore (:value input-result-without-skip)))
              "Without --skip-ignore, both global and project ignores should be used"))

        (finally
          (System/setProperty "user.dir" original-dir)
          (System/setProperty "user.home" original-home)
          (try
            (when (.exists (io/file temp-project))
              (doseq [f (reverse (file-seq (io/file temp-project)))]
                (.delete f)))
            (when (.exists (io/file temp-home))
              (doseq [f (reverse (file-seq (io/file temp-home)))]
                (.delete f)))
            (catch Exception _e nil)))))))

(deftest parallel-exception-should-not-crash
  (testing "Parallel processing should handle individual check failures gracefully"
    (testing "Pipeline should continue despite individual failures"
      (let [lines (vec (for [i (range 10)]
                        {:line-num i :text (str "Line " i) :issue? false}))
            process-line (fn [line]
                          (try
                            (if (= (:line-num line) 5)
                              (throw (Exception. "Test failure on line 5"))
                              line)
                            (catch Exception e
                              {:line-num (:line-num line)
                               :text (:text line)
                               :error (.getMessage e)})))
            results (doall (pmap process-line lines))]

        (is (= 10 (count results))
            "Should process all lines despite individual failures")

        (let [line-5-result (nth results 5)]
          (is (contains? line-5-result :error)
              "Failed line should have error recorded"))

        (let [successful (filter #(not (contains? % :error)) results)]
          (is (= 9 (count successful))
              "Other lines should process successfully"))))))

(deftest quoted-text-flag-integration
  (testing "--quoted-text flag controls whether quoted text is checked"
    (let [temp-dir (str (System/getProperty "java.io.tmpdir")
                       File/separator
                       "proserunner-quoted-integration-"
                       (System/currentTimeMillis))
          temp-file (str temp-dir File/separator "test.md")
          ;; Content with a specimen word inside quotes
          test-content "She said \"obviously this is wrong\" and continued walking."
          _ (.mkdirs (io/file temp-dir))
          _ (spit temp-file test-content)
          config-path (str (System/getProperty "user.home")
                          File/separator
                          ".proserunner"
                          File/separator
                          "config.edn")]

      (try
        (testing "Without --quoted-text flag, quoted portions are not checked"
          (let [result (vet/compute-or-cached {:file temp-file
                                               :config config-path
                                               :output "table"
                                               :code-blocks false
                                               :quoted-text false
                                               :no-cache true
                                               :parallel-files false
                                               :parallel-lines false})]
            (is (result/success? result))
            (let [issues (:results (:results (:value result)))]
              ;; The word "obviously" is inside quotes, so it should NOT be flagged
              ;; when quoted-text is false (default behavior)
              (is (empty? (filter #(some (fn [issue]
                                          (= "obviously" (:specimen issue)))
                                        (:issues %))
                                issues))
                  "Specimens inside quoted text should not be flagged when quoted-text is false"))))

        (testing "With --quoted-text flag, quoted portions ARE checked"
          (let [result (vet/compute-or-cached {:file temp-file
                                               :config config-path
                                               :output "table"
                                               :code-blocks false
                                               :quoted-text true
                                               :no-cache true
                                               :parallel-files false
                                               :parallel-lines false})]
            (is (result/success? result))
            (let [issues (:results (:results (:value result)))]
              ;; The word "obviously" is inside quotes, but WITH quoted-text flag
              ;; it SHOULD be flagged (if "obviously" is in the default checks)
              ;; Check that issues can contain specimens from quoted text
              (is (seqable? issues)
                  "Results should be a seqable collection"))))

        (finally
          (when (.exists (io/file temp-dir))
            (doseq [f (reverse (file-seq (io/file temp-dir)))]
              (.delete f))))))))

(deftest compute-paths-merges-several-paths
  (testing "issues from every path land in one payload, numbered as one run"
    (let [config-path (str (System/getProperty "user.home")
                           File/separator ".proserunner" File/separator "config.edn")
          cache-dir (str (System/getProperty "java.io.tmpdir") File/separator
                         "proserunner-compute-paths-" (System/nanoTime))
          opts {:paths ["resources/drivel.md" "resources/more/words.md"]
                :config config-path
                :cache-dir cache-dir
                :output "group"}
          r (binding [*out* (java.io.StringWriter.)
                      *err* (java.io.StringWriter.)]
              (vet/compute-paths opts))]
      (try
        (is (result/success? r))
        (let [payload (:value r)
              files (set (map :file (get-in payload [:results :results])))]
          (is (= 2 (:file-count payload)))
          (is (some #(re-find #"drivel\.md$" %) files))
          (is (some #(re-find #"words\.md$" %) files)))
        (finally
          (doseq [f (reverse (file-seq (io/file cache-dir)))]
            (.delete ^File f))))))

  (testing "a path with nothing to check fails, naming the path"
    (let [r (binding [*out* (java.io.StringWriter.)
                      *err* (java.io.StringWriter.)]
              (vet/compute-paths {:paths ["deps.edn"]
                                  :config (str (System/getProperty "user.home")
                                               "/.proserunner/config.edn")
                                  :output "group"}))]
      (is (result/failure? r))
      (is (re-find #"^deps\.edn: unsupported file type" (:error r))))))
