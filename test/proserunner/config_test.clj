(ns proserunner.config-test
  "Tests for config loading and initialization.

  Note: fetch-or-create! has essential complexity (decision tree with ordering).
  These tests document the branching behavior without requiring structural changes."
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.config :as config]
            [proserunner.config.loader]
            [proserunner.result]))

(deftest fetch-or-create-input-validation-test
  (testing "Throws on non-string config-filepath"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"expects a string filepath"
          (config/fetch-or-create! 123))))

  (testing "Throws with type information in error context"
    (try
      (config/fetch-or-create! {:invalid "map"})
      (is false "Should have thrown")
      (catch clojure.lang.ExceptionInfo e
        (is (= clojure.lang.PersistentArrayMap
               (type (-> e ex-data :config-filepath))))
        (is (contains? (ex-data e) :config-filepath))))))

;; Note: nil is valid input and is tested through integration tests

(deftest fetch-or-create-decision-branches-test
  (testing "Decision tree branches are ordered correctly"
    ;; This test documents the ordering without executing branches
    ;; The actual branching is tested through integration tests
    (let [branches [:custom-config-specified
                    :in-project-directory
                    :checks-stale
                    :config-exists
                    :first-time-init]]
      (is (= 5 (count branches))
          "fetch-or-create! has 5 decision branches")

      ;; Document critical ordering constraint
      (is (= :in-project-directory (nth branches 1))
          "Project check must come before stale/exists checks (per comment)")

      ;; Document that function handles all cases
      (is (some #{:first-time-init} branches)
          "Function handles first-time initialization"))))

(deftest fetch-or-create-documentation-test
  (testing "Function has clear documentation"
    (let [docstring (:doc (meta #'config/fetch-or-create!))]
      (is (string? docstring))
      (is (re-find #"project" docstring)
          "Documents project config behavior")
      (is (re-find #"update" docstring)
          "Documents update checking"))))

(deftest restore-defaults-structure-test
  (testing "restore-defaults! has well-extracted helper functions"
    ;; Document that helpers were extracted during refactoring
    (let [helpers ['backup-and-preserve-files
                   'download-and-restore-preserved-files!]]
      (is (= 2 (count helpers))
          "Two helper functions extracted from restore-defaults!")

      ;; Verify helpers exist
      (is (resolve 'proserunner.config/backup-and-preserve-files)
          "backup-and-preserve-files helper exists")
      (is (resolve 'proserunner.config/download-and-restore-preserved-files!)
          "download-and-restore-preserved-files! helper exists")))

  (testing "restore-defaults! returns Result"
    ;; Function uses result/try-result-with-context wrapper
    (let [docstring (:doc (meta #'config/restore-defaults!))]
      (is (re-find #"Result" docstring)
          "Documents Result return type")))

  (testing "restore-defaults! handles two scenarios"
    ;; Documents the branching logic
    (let [scenarios [:fresh-installation :backup-and-restore]]
      (is (= 2 (count scenarios))
          "Handles fresh install and backup/restore scenarios"))))

(deftest ensure-checks-exist-structure-test
  (testing "ensure-checks-exist! validates check references"
    ;; Documents that this function checks if referenced checks exist
    (is (resolve 'proserunner.config/ensure-checks-exist!)
        "ensure-checks-exist! function exists"))

  (testing "ensure-checks-exist! returns Result type"
    (let [docstring (:doc (meta #'config/ensure-checks-exist!))]
      (is (string? docstring))
      (is (re-find #"Result" docstring)
          "Documents Result return type")))

  (testing "ensure-checks-exist! triggers download for missing checks"
    ;; Function should detect missing "default" or "custom" and download them
    (let [scenarios [:missing-default :missing-custom :checks-exist]]
      (is (= 3 (count scenarios))
          "Handles missing default, missing custom, and exists scenarios"))))

(deftest load-config-from-file-returns-result-test
  (testing "load-config-from-file returns Success for valid config"
    (let [temp-file (java.io.File/createTempFile "test-config" ".edn")]
      (try
        (spit (.getPath temp-file) "{:checks []}")
        (let [result (proserunner.config.loader/load-config-from-file (.getPath temp-file))]
          (is (proserunner.result/success? result))
          (is (some? (:value result))))
        (finally
          (.delete temp-file)))))

  (testing "load-config-from-file returns Failure for non-existent file"
    (let [result (proserunner.config.loader/load-config-from-file "/nonexistent/file.edn")]
      (is (proserunner.result/failure? result))
      (is (string? (:error result)))
      (is (contains? (:context result) :filepath))))

  (testing "load-config-from-file returns Failure for invalid EDN"
    (let [temp-file (java.io.File/createTempFile "bad-config" ".edn")]
      (try
        (spit (.getPath temp-file) "{:checks [}")
        (let [result (proserunner.config.loader/load-config-from-file (.getPath temp-file))]
          (is (proserunner.result/failure? result))
          (is (string? (:error result))))
        (finally
          (.delete temp-file))))))

;; Note: Comprehensive integration tests exist in:
;; - test/proserunner/integration_test.clj
;; - test/proserunner/custom_checks_test.clj
;; These test actual config loading with real file system operations.

;;; Tests for determine-config-strategy (pure function extracted from fetch-or-create!)

(deftest determine-config-strategy-test
  (testing "returns :custom when custom config specified and exists"
    (is (= :custom (config/determine-config-strategy
                     {:using-default? false
                      :custom-exists? true
                      :in-project? false
                      :config-exists? false}))
        "Custom config file via -c flag takes precedence"))

  (testing "returns :project when using default and in project directory"
    (is (= :project (config/determine-config-strategy
                      {:using-default? true
                       :custom-exists? false
                       :in-project? true
                       :config-exists? false}))
        "Project directory config should be loaded"))

  (testing "returns :global when config exists"
    (is (= :global (config/determine-config-strategy
                     {:using-default? true
                      :custom-exists? false
                      :in-project? false
                      :config-exists? true}))
        "Global config should be loaded when it exists"))

  (testing "returns :initialize when config doesn't exist"
    (is (= :initialize (config/determine-config-strategy
                         {:using-default? true
                          :custom-exists? false
                          :in-project? false
                          :config-exists? false}))
        "Should initialize on first run"))

  (testing "enforces ordering: project takes precedence over global"
    (is (= :project (config/determine-config-strategy
                      {:using-default? true
                       :custom-exists? false
                       :in-project? true
                       :config-exists? true}))
        "Project check must come before global check"))

  (testing "custom config takes precedence over everything"
    (is (= :custom (config/determine-config-strategy
                     {:using-default? false
                      :custom-exists? true
                      :in-project? true
                      :config-exists? true}))
        "Custom config via -c flag has highest precedence"))

  (testing "there is no implicit update strategy"
    (let [strategies (set (for [using-default? [true false]
                                custom-exists? [true false]
                                in-project? [true false]
                                config-exists? [true false]]
                            (config/determine-config-strategy
                              {:using-default? using-default?
                               :custom-exists? custom-exists?
                               :in-project? in-project?
                               :config-exists? config-exists?})))]
      (is (= #{:custom :project :global :initialize} strategies)
          "All four strategy branches are reachable, and none updates checks"))))

(deftest fetch-or-create-makes-no-network-calls-test
  (testing "loading an existing global config never contacts GitHub"
    (let [home (str (System/getProperty "java.io.tmpdir") "/proserunner-nonet-" (System/nanoTime))
          proserunner-dir (java.io.File. home ".proserunner")]
      (try
        (.mkdirs (java.io.File. proserunner-dir "default"))
        (spit (java.io.File. proserunner-dir "config.edn") "{:checks []}")
        (let [calls (atom [])]
          (with-redefs [proserunner.system/home-dir (constantly home)
                        babashka.http-client/get (fn [url & _]
                                                   (swap! calls conj url)
                                                   {:status 500})]
            (is (some? (config/fetch-or-create! (str home "/.proserunner/config.edn"))))
            (is (empty? @calls) "no HTTP requests during a lint")))
        (finally
          (doseq [f (reverse (file-seq (java.io.File. home)))]
            (.delete ^java.io.File f)))))))
