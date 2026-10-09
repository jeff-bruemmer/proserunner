(ns proserunner.custom-checks-test
  (:require [proserunner.custom-checks :as custom]
            [proserunner.ignore.context :as ignore-context]
            [proserunner.ignore.file :as ignore-file]
            [proserunner.project-config :as project-config]
            [proserunner.system :as sys]
            [proserunner.test-helpers :refer [delete-recursively temp-dir-path silently]]
            [clojure.string :as string]
            [clojure.test :as t :refer [deftest is testing use-fixtures]]
            [clojure.java.io :as io]
            [clojure.edn :as edn])
  (:import java.io.File))

(def original-home (atom nil))
(def test-home (atom nil))
(def test-project-root (atom nil))

(defn setup-test-env [f]
  ;; Save original home
  (reset! original-home (System/getProperty "user.home"))

  ;; Create temporary home directory
  (let [temp-home (temp-dir-path "proserunner-test-home")
        proserunner-dir (str temp-home File/separator ".proserunner")
        temp-project (temp-dir-path "proserunner-test-project")]
    (reset! test-home temp-home)
    (reset! test-project-root temp-project)

    ;; Set user.home to temp directory
    (System/setProperty "user.home" temp-home)

    ;; Create test directory structures
    (.mkdirs (io/file proserunner-dir "custom"))
    (.mkdirs (io/file temp-project))

    ;; Create a mock config.edn
    (spit (str proserunner-dir File/separator "config.edn")
          "{:checks []}")

    ;; Run the test with suppressed output
    (try
      (silently (f))
      (finally
        ;; Restore original home
        (System/setProperty "user.home" @original-home)

        ;; Clean up temp directories
        (delete-recursively (io/file temp-home))
        (delete-recursively (io/file temp-project))))))

(use-fixtures :each setup-test-env)

(deftest add-checks-from-directory
  (testing "Adding checks from a local directory"
    (let [;; Create source directory with test check files
          source-dir (str @test-home File/separator "source-checks")
          _ (.mkdirs (io/file source-dir))

          ;; Create test check files
          _ (spit (str source-dir File/separator "test1.edn")
                  "{:name \"test1\" :kind \"existence\" :specimens [\"word\"]}")
          _ (spit (str source-dir File/separator "test2.edn")
                  "{:name \"test2\" :kind \"recommender\" :recommendations []}")

          ;; Add checks
          result (custom/add-checks source-dir {})]

      ;; Verify result structure
      (is (= 2 (:count result)))
      (is (= #{"test1" "test2"} (set (:check-names result))))

      ;; Verify files were copied
      (is (.exists (io/file (:target-dir result) "test1.edn")))
      (is (.exists (io/file (:target-dir result) "test2.edn")))

      ;; Verify config was updated
      (let [config-path (sys/config-path "config.edn")
            config (edn/read-string (slurp config-path))]
        (is (= 1 (count (:checks config))))
        (is (= "Custom checks: source-checks" (-> config :checks first :name)))
        (is (= "custom/source-checks" (-> config :checks first :directory)))
        (is (= #{"test1" "test2"} (set (-> config :checks first :files))))))))

(deftest add-checks-with-custom-name
  (testing "Adding checks with a custom directory name"
    (let [;; Create source directory
          source-dir (str @test-home File/separator "some-checks")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "check.edn")
                  "{:name \"check\" :kind \"existence\" :specimens []}")

          ;; Add with custom name
          result (custom/add-checks source-dir {:name "my-custom-name"})]

      ;; Verify custom name was used
      (is (clojure.string/includes? (:target-dir result) "my-custom-name"))
      (is (.exists (io/file (:target-dir result) "check.edn")))

      ;; Verify config uses custom name
      (let [config-path (sys/config-path "config.edn")
            config (edn/read-string (slurp config-path))]
        (is (= "custom/my-custom-name" (-> config :checks first :directory)))))))

(deftest add-checks-no-edn-files
  (testing "Error when directory has no .edn files"
    (let [;; Create empty directory
          source-dir (str @test-home File/separator "empty-dir")
          _ (.mkdirs (io/file source-dir))]

      ;; Should throw exception with improved error message
      (is (thrown-with-msg?
           clojure.lang.ExceptionInfo
           #"No .edn check files found"
           (custom/add-checks source-dir {}))))))

(deftest add-checks-updates-existing-entry
  (testing "Updating existing check directory in config"
    (let [;; Create source directory
          source-dir (str @test-home File/separator "update-test")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "new-check.edn")
                  "{:name \"new\" :kind \"existence\" :specimens []}")

          ;; Create initial config with existing entry
          config-path (sys/config-path "config.edn")
          _ (spit config-path
                  "{:checks [{:name \"Custom checks: update-test\"
                              :directory \"custom/update-test\"
                              :files [\"old-check\"]}]}")

          ;; Add checks (should update existing entry)
          _ (custom/add-checks source-dir {})
          config (edn/read-string (slurp config-path))]

      ;; Verify config was updated (not duplicated)
      (is (= 1 (count (:checks config))))
      (is (= ["new-check"] (-> config :checks first :files))))))

(deftest extract-name-from-source
  (testing "Extracting name from local directory paths"
    (is (= "checks" (#'custom/extract-name-from-source "/path/to/checks")))
    (is (= "checks" (#'custom/extract-name-from-source "/path/to/checks/")))
    (is (= "my-checks" (#'custom/extract-name-from-source "~/my-checks")))
    (is (= "my-checks" (#'custom/extract-name-from-source "./my-checks")))))

;;; Context-Aware Target Determination Tests

(deftest determine-target-no-flags-no-project
  (testing "No flags + no project -> returns :global"
    (let [start-dir @test-project-root
          result (project-config/determine-target {} start-dir)]
      (is (= :global result)))))

(deftest determine-target-no-flags-with-project
  (testing "No flags + project exists -> returns :project"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"]}")
          result (project-config/determine-target {} project-root)]
      (is (= :project result)))))

(deftest determine-target-global-flag
  (testing "Global flag always returns :global"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"]}")
          ;; Even with project, --global forces global
          result (project-config/determine-target {:global true} project-root)]
      (is (= :global result)))))

(deftest determine-target-project-flag-with-project
  (testing "Project flag + project exists -> returns :project"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"]}")
          result (project-config/determine-target {:project true} project-root)]
      (is (= :project result)))))

(deftest determine-target-project-flag-no-project-fails
  (testing "Project flag + no project -> throws error"
    (let [start-dir @test-project-root]
      (is (thrown-with-msg?
           clojure.lang.ExceptionInfo
           #"No project configuration found"
           (project-config/determine-target {:project true} start-dir))))))

(deftest determine-target-both-flags-fails
  (testing "Both --global and --project -> throws error"
    (is (thrown-with-msg?
         clojure.lang.ExceptionInfo
         #"Cannot specify both"
         (project-config/determine-target {:global true :project true} @test-project-root)))))

(deftest determine-target-from-subdirectory
  (testing "Finds project from nested subdirectory"
    (let [project-root @test-project-root
          sub-dir (str project-root File/separator "src" File/separator "nested")
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file sub-dir))
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"]}")
          result (project-config/determine-target {} sub-dir)]
      (is (= :project result)))))

;;; Add-Checks with Context-Aware Targeting Tests

(deftest add-checks-to-project-default-behavior
  (testing "Adding checks to project when .proserunner exists (no flags)"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          checks-dir (str proserunner-dir File/separator "checks")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file checks-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{} :ignore-mode :extend :config-mode :merged}")

          ;; Create source checks
          source-dir (str @test-home File/separator "source-checks")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "test1.edn")
                  "{:name \"test1\" :kind \"existence\" :specimens [\"word\"]}")

          ;; Add checks from project directory
          result (custom/add-checks source-dir {:start-dir project-root})]

      ;; Should add to project
      (is (= :project (:target result)))
      ;; Check files copied to .proserunner/checks/
      (is (string/includes? (:target-dir result) ".proserunner"))
      (is (.exists (io/file (:target-dir result) "test1.edn"))))))

(deftest add-checks-global-flag-forces-global
  (testing "Global flag forces checks to go to ~/.proserunner/custom/"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"]}")

          ;; Create source checks
          source-dir (str @test-home File/separator "source-checks-global")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "test1.edn")
                  "{:name \"test1\" :kind \"existence\" :specimens [\"word\"]}")

          ;; Add checks with --global flag
          result (custom/add-checks source-dir {:global true :start-dir project-root})]

      ;; Should add to global despite project existing
      (is (= :global (:target result)))
      (is (string/includes? (:target-dir result) "custom"))
      (is (not (string/includes? (:target-dir result) ".proserunner/checks"))))))

(deftest add-checks-project-flag-requires-project
  (testing "Project flag fails when no project initialized"
    (let [source-dir (str @test-home File/separator "source-checks-proj")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "test1.edn")
                  "{:name \"test1\" :kind \"existence\" :specimens [\"word\"]}")]

      (is (thrown-with-msg?
           clojure.lang.ExceptionInfo
           #"No project configuration found"
           (custom/add-checks source-dir {:project true :start-dir @test-project-root}))))))

(deftest add-checks-project-updates-project-config
  (testing "Project-scoped checks update .proserunner/config.edn"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          checks-dir (str proserunner-dir File/separator "checks")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file checks-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{} :ignore-mode :extend :config-mode :merged}")

          ;; Create source checks
          source-dir (str @test-home File/separator "company-checks")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "terms.edn")
                  "{:name \"terms\" :kind \"recommender\" :specimens {}}")

          ;; Add checks to project
          _ (custom/add-checks source-dir {:start-dir project-root})

          ;; Verify project config updated
          config (edn/read-string (slurp manifest-path))]
      (is (some #(or (= % "checks")
                     (and (map? %) (= (:directory %) "checks")))
                (:checks config))))))

(deftest add-checks-global-updates-global-config
  (testing "Global checks update ~/.proserunner/config.edn"
    (let [;; Create source checks
          source-dir (str @test-home File/separator "global-checks")
          _ (.mkdirs (io/file source-dir))
          _ (spit (str source-dir File/separator "test1.edn")
                  "{:name \"test1\" :kind \"existence\" :specimens [\"word\"]}")

          ;; Add checks globally
          _ (custom/add-checks source-dir {:global true})

          ;; Verify global config updated
          config-path (sys/config-path "config.edn")
          config (edn/read-string (slurp config-path))]
      (is (some #(= "custom/global-checks" (:directory %)) (:checks config))))))

;;; Ignore Management with Context-Aware Targeting Tests

(deftest add-ignore-to-global-default
  (testing "Adding ignore to global when no project exists"
    (let [_ (ignore-context/add! "hopefully" {})
          ignored (ignore-file/read)]
      (is (contains? (:ignore ignored) "hopefully")))))

(deftest add-ignore-to-project-when-exists
  (testing "Adding ignore to project when .proserunner exists"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{} :ignore-mode :extend :config-mode :merged}")

          _ (ignore-context/add! "TODO" {:start-dir project-root})

          ;; Verify added to project config
          config (edn/read-string (slurp manifest-path))]
      (is (contains? (:ignore config) "TODO")))))

(deftest add-ignore-global-flag-forces-global
  (testing "Global flag forces ignore to global file"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{} :ignore-mode :extend :config-mode :merged}")

          _ (ignore-context/add! "FIXME" {:global true :start-dir project-root})

          ;; Verify added to global, not project
          global-ignored (ignore-file/read)
          config (edn/read-string (slurp manifest-path))]
      (is (contains? (:ignore global-ignored) "FIXME"))
      (is (not (contains? (:ignore config) "FIXME"))))))

(deftest add-ignore-project-flag-requires-project
  (testing "Project flag fails when no project initialized"
    (is (thrown-with-msg?
         clojure.lang.ExceptionInfo
         #"No project configuration found"
         (ignore-context/add! "term" {:project true :start-dir @test-project-root})))))

(deftest remove-ignore-from-project
  (testing "Removing ignore from project config"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{\"TODO\" \"FIXME\"} :ignore-mode :extend :config-mode :merged}")

          _ (ignore-context/remove! "TODO" {:start-dir project-root})

          ;; Verify removed from project
          config (edn/read-string (slurp manifest-path))]
      (is (not (contains? (:ignore config) "TODO")))
      (is (contains? (:ignore config) "FIXME")))))

(deftest list-ignored-shows-both-contexts
  (testing "Listing ignores shows both global and project"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{\"TODO\"} :ignore-mode :extend :config-mode :merged}")

          ;; Add to global
          _ (ignore-context/add! "global-term" {:global true})

          ;; List from project context
          result (ignore-context/list {:start-dir project-root})]

      ;; Should show both global and project ignores when in :extend mode
      (is (contains? (:ignore result) "global-term"))
      (is (contains? (:ignore result) "TODO")))))

(deftest clear-ignored-context-aware
  (testing "Clearing ignores respects context"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file proserunner-dir))
          _ (spit manifest-path "{:checks [\"default\"] :ignore #{\"TODO\" \"FIXME\"} :ignore-mode :extend :config-mode :merged}")

          ;; Clear project ignores
          _ (ignore-context/clear! {:start-dir project-root})

          ;; Verify project ignores cleared
          config (edn/read-string (slurp manifest-path))]
      (is (empty? (:ignore config))))))

;;; Config Loading Tests - Verify --checks shows project checks

(deftest config-loading-shows-project-checks
  (testing "fetch-or-create! loads project config when in project directory"
    (let [project-root @test-project-root
          proserunner-dir (str project-root File/separator ".proserunner")
          checks-dir (str proserunner-dir File/separator "checks")
          manifest-path (str proserunner-dir File/separator "config.edn")
          _ (.mkdirs (io/file checks-dir))

          ;; Create project with custom check
          _ (spit manifest-path "{:checks [\"default\" {:directory \"checks\"}] :ignore #{} :ignore-mode :extend :config-mode :merged}")
          _ (spit (str checks-dir File/separator "custom-check.edn")
                  "{:name \"custom-test-check\" :kind \"existence\" :specimens [\"testword\"] :message \"Test\" :explanation \"Test check\"}")

          ;; Change to project directory and load config
          original-dir (System/getProperty "user.dir")
          _ (System/setProperty "user.dir" project-root)

          ;; Require config namespace and get fetch-or-create function
          _ (require 'proserunner.config)
          fetch-or-create (resolve 'proserunner.config/fetch-or-create!)

          ;; Load config (should detect project and load project config)
          loaded-config (fetch-or-create nil)

          ;; Restore original directory
          _ (System/setProperty "user.dir" original-dir)]

      ;; Verify project checks are included
      (is (some? loaded-config))
      (is (some #(string/includes? (str %) "custom-check")
                (mapcat :files (:checks loaded-config)))
          "Project-specific checks should be loaded"))))
