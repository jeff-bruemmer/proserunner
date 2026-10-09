(ns proserunner.vet.input-test
  (:require [clojure.test :refer [deftest is testing use-fixtures]]
            [proserunner.vet.input :as input]
            [proserunner.checks]
            [proserunner.config]
            [proserunner.project-config]
            [proserunner.result :as result]
            [proserunner.storage]
            [proserunner.system]
            [proserunner.test-helpers :refer [delete-recursively temp-dir-path silently with-system-property]]
            [clojure.java.io :as io]
            [clojure.string]
            [editors.registry :as registry]
            [editors.utilities :as util]
            [editors.repetition :as repetition]
            [editors.re :as re])
  (:import java.io.File))

(def test-temp-dir (atom nil))

(defn setup-test-env [f]
  (let [temp-dir (temp-dir-path "proserunner-input-test")]
    (reset! test-temp-dir temp-dir)
    (doseq [editor-type (util/standard-editor-types)]
      (registry/register-editor! editor-type (util/create-editor editor-type)))
    (registry/register-editor! "repetition" repetition/proofread)
    (registry/register-editor! "regex" re/proofread)
    (try
      (silently (f))
      (finally
        (delete-recursively (io/file temp-dir))))))

(use-fixtures :each setup-test-env)

(deftest build-ignore-patterns-test
  (testing "Builds ignore patterns from file directory"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          ignore-info (#'input/build-ignore-patterns temp-dir [])]
      (is (map? ignore-info))
      (is (contains? ignore-info :base-dir))
      (is (contains? ignore-info :patterns)))))

(deftest filter-valid-files-test
  (testing "Filters for supported file types"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "test.md") "# Test")
          _ (spit (str temp-dir File/separator "test.txt") "Test")
          _ (spit (str temp-dir File/separator "test.jpg") "binary")
          ignore-info {:base-dir (.getAbsolutePath (io/file temp-dir))
                       :patterns []}
          files-result (#'input/filter-valid-files temp-dir ignore-info)]
      (is (result/success? files-result))
      (let [files (:value files-result)]
        (is (seq files))
        (is (every? #(or (re-find #"\.md$" %)
                        (re-find #"\.txt$" %))
                   files))))))

(deftest determine-parallel-settings-test
  (testing "Determines parallel settings from options"
    (is (= {:parallel-files? true :parallel-lines? false}
           (#'input/determine-parallel-settings {:parallel-files true}))
        "parallel files turns off parallel lines")
    (is (= {:parallel-files? false :parallel-lines? false}
           (#'input/determine-parallel-settings {:sequential-lines true})))
    (is (= {:parallel-files? false :parallel-lines? true}
           (#'input/determine-parallel-settings {})))))

(deftest make-input-test
  (testing "make-input creates Input record from options"
    (let [temp-dir @test-temp-dir
          config-path (str (System/getProperty "user.home")
                          File/separator ".proserunner"
                          File/separator "config.edn")
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "test.md") "# Test file\n\nSome content.")
          opts {:file temp-dir
                :config config-path
                :output "table"
                :code-blocks false
                :quoted-text false
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result))
      (let [input (:value input-result)]
        (is (= (:file input) temp-dir))
        (is (seq (:lines input)))
        (is (some? (:config input)))
        (is (some? (:checks input)))))))

(deftest get-lines-from-files-parallel
  (testing "Gets lines from files in parallel"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "file1.md") "Line 1")
          _ (spit (str temp-dir File/separator "file2.md") "Line 2")
          lines-result (#'input/get-lines-from-all-files
                        {:code-blocks false
                         :check-quoted-text false
                         :file temp-dir
                         :exclude-patterns []
                         :parallel? true})]
      (is (result/success? lines-result))
      (let [lines (:value lines-result)]
        (is (= 2 (count lines)))))))

(deftest get-lines-from-files-sequential
  (testing "Gets lines from files sequentially"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "file1.md") "Line 1")
          lines-result (#'input/get-lines-from-all-files
                        {:code-blocks false
                         :check-quoted-text false
                         :file temp-dir
                         :exclude-patterns []
                         :parallel? false})]
      (is (result/success? lines-result))
      (let [lines (:value lines-result)]
        (is (= 1 (count lines)))))))

(deftest make-input-with-exclude-patterns
  (testing "make-input respects exclude patterns"
    (let [temp-dir @test-temp-dir
          config-path (str (System/getProperty "user.home")
                          File/separator ".proserunner"
                          File/separator "config.edn")
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "include.md") "Include this")
          _ (spit (str temp-dir File/separator "exclude.md") "Exclude this")
          opts {:file temp-dir
                :config config-path
                :output "table"
                :code-blocks false
                :quoted-text false
                :exclude "exclude.md"
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result)))))

(deftest make-input-with-multiple-exclude-patterns
  (testing "make-input respects multiple exclude patterns"
    (let [temp-dir @test-temp-dir
          config-path (str (System/getProperty "user.home")
                          File/separator ".proserunner"
                          File/separator "config.edn")
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "include.md") "Include this")
          _ (spit (str temp-dir File/separator "exclude1.md") "Exclude this")
          _ (spit (str temp-dir File/separator "exclude2.md") "Exclude this too")
          _ (spit (str temp-dir File/separator "skip.txt") "Skip this")
          opts {:file temp-dir
                :config config-path
                :output "table"
                :code-blocks false
                :quoted-text false
                :exclude ["exclude1.md" "exclude2.md" "skip.txt"]
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result))
      (let [input (:value input-result)
            lines (:lines input)]
        ;; Should only contain lines from include.md
        (is (pos? (count lines)))
        (is (every? #(clojure.string/includes? (:file %) "include.md") lines))))))

(deftest make-input-with-directory-and-file-excludes
  (testing "make-input can exclude both directories and files"
    (let [temp-dir @test-temp-dir
          subdir (str temp-dir File/separator "drafts")
          _ (.mkdirs (io/file subdir))
          _ (spit (str temp-dir File/separator "keep.md") "Keep this")
          _ (spit (str temp-dir File/separator "skip.md") "Skip this")
          _ (spit (str subdir File/separator "draft.md") "Draft content")
          opts {:file temp-dir
                :config (str (System/getProperty "user.home")
                            File/separator ".proserunner"
                            File/separator "config.edn")
                :output "table"
                :code-blocks false
                :quoted-text false
                :exclude ["drafts/*" "skip.md"]
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result))
      (let [input (:value input-result)
            lines (:lines input)]
        ;; Should only contain lines from keep.md
        (is (pos? (count lines)))
        (is (every? #(clojure.string/includes? (:file %) "keep.md") lines))))))

(deftest make-input-with-empty-exclude
  (testing "make-input handles empty exclude list"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "file.md") "Content")
          opts {:file temp-dir
                :config (str (System/getProperty "user.home")
                            File/separator ".proserunner"
                            File/separator "config.edn")
                :output "table"
                :code-blocks false
                :quoted-text false
                :exclude []
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result))
      (let [input (:value input-result)]
        (is (seq (:lines input)))))))

(deftest make-input-with-wildcard-excludes
  (testing "make-input handles wildcard patterns in excludes"
    (let [temp-dir @test-temp-dir
          _ (.mkdirs (io/file temp-dir))
          _ (spit (str temp-dir File/separator "keep.md") "Keep this")
          _ (spit (str temp-dir File/separator "temp.backup") "Backup")
          _ (spit (str temp-dir File/separator "draft.backup") "Draft backup")
          opts {:file temp-dir
                :config (str (System/getProperty "user.home")
                            File/separator ".proserunner"
                            File/separator "config.edn")
                :output "table"
                :code-blocks false
                :quoted-text false
                :exclude ["*.backup"]
                :parallel-files false}
          input-result (input/make opts)]
      (is (result/success? input-result))
      (let [input (:value input-result)
            lines (:lines input)]
        (is (pos? (count lines)))
        (is (every? #(clojure.string/includes? (:file %) "keep.md") lines))))))

(deftest make-input-loads-project-ignore-patterns
  (testing "Project config ignore patterns are loaded into Input"
    (with-system-property "user.dir" @test-temp-dir
      (fn []
        (let [temp-dir @test-temp-dir
              proserunner-dir (io/file temp-dir ".proserunner")
              _ (.mkdirs proserunner-dir)
              ;; Create project config with ignore patterns - use "default" checks from global
              _ (spit (str proserunner-dir File/separator "config.edn")
                      (pr-str {:checks ["default"]
                               :ignore #{"adverb" "cliche"}
                               :ignore-mode :extend}))
              _ (spit (str temp-dir File/separator "test.md") "# Test")
              opts {:file temp-dir
                    :config (str (System/getProperty "user.home")
                                File/separator ".proserunner"
                                File/separator "config.edn")
                    :output "table"
                    :parallel-files false}
              input-result (input/make opts)]
          (is (result/success? input-result))
          (let [input (:value input-result)]
            ;; Verify ignore is a set, not a string
            (is (set? (-> input :config :ignore)))
            ;; Verify correct ignore patterns loaded
            (is (= #{"adverb" "cliche"} (-> input :config :ignore)))
            ;; Verify project-ignore also populated
            (is (= #{"adverb" "cliche"} (:project-ignore input)))))))))

(deftest load-config-and-dir-calls-fetch-or-create-test
  (testing "load-config-and-dir should call fetch-or-create! for both project and global configs"
    ;; This ensures auto-download logic runs for all config types
    (let [fetch-called (atom false)
          mock-config {:checks [] :ignore #{}}
          mock-project-config {:source :project :checks [] :ignore #{}}]

      ;; Mock fetch-or-create! to track if it's called
      (with-redefs [proserunner.config/fetch-or-create!
                    (fn [_]
                      (reset! fetch-called true)
                      mock-config)]

        ;; Test with project config (source = :project)
        (reset! fetch-called false)
        (let [result (#'input/load-config-and-dir nil mock-project-config)]
          (is @fetch-called
              "fetch-or-create! should be called for project configs")
          (is (some? (:config result)))
          (is (= "" (:check-dir result))))

        ;; Test with global config (source != :project)
        (reset! fetch-called false)
        (let [non-project-config (assoc mock-project-config :source :global)
              result (#'input/load-config-and-dir nil non-project-config)]
          (is @fetch-called
              "fetch-or-create! should be called for global configs")
          (is (some? (:config result)))))))

  (testing "fetch-or-create! is called with nil for project configs to use default path"
    ;; Project configs should use the default config path for auto-download
    (let [config-arg (atom nil)
          mock-config {:checks [] :ignore #{}}
          mock-project-config {:source :project :checks [] :ignore #{}}]

      (with-redefs [proserunner.config/fetch-or-create!
                    (fn [config]
                      (reset! config-arg config)
                      mock-config)]

        ;; For project config, should call with nil to use default path
        (reset! config-arg :not-called)
        (#'input/load-config-and-dir "custom-config" mock-project-config)
        (is (nil? @config-arg)
            "Should call fetch-or-create! with nil for project configs")

        ;; For non-project config, should pass through the config arg
        (reset! config-arg :not-called)
        (let [non-project-config (assoc mock-project-config :source :global)]
          (#'input/load-config-and-dir "custom-config" non-project-config)
          (is (= "custom-config" @config-arg)
              "Should pass config arg through for non-project configs"))))))

;;; Tests for normalize-input-options (pure function extracted from make)

(deftest normalize-input-options-test
  (testing "normalizes vector exclude patterns"
    (let [opts {:exclude ["*.log" "temp/**"]}
          normalized (input/normalize-input-options opts)]
      (is (= ["*.log" "temp/**"] (:exclude-patterns normalized)))))

  (testing "normalizes string exclude to vector"
    (let [opts {:exclude "*.log"}
          normalized (input/normalize-input-options opts)]
      (is (= ["*.log"] (:exclude-patterns normalized)))))

  (testing "handles nil exclude"
    (let [opts {}
          normalized (input/normalize-input-options opts)]
      (is (= [] (:exclude-patterns normalized)))))

  (testing "handles nil exclude with other options"
    (let [opts {:file "test.md"}
          normalized (input/normalize-input-options opts)]
      (is (= [] (:exclude-patterns normalized)))))

  (testing "determines parallel settings correctly - both enabled"
    (let [opts {:parallel-files true :sequential-lines false}
          normalized (input/normalize-input-options opts)]
      (is (true? (:parallel-files? normalized)))
      (is (false? (:sequential-lines? normalized)))))

  (testing "sequential-lines disables parallel-lines"
    (let [opts {:sequential-lines true}
          normalized (input/normalize-input-options opts)]
      (is (false? (:parallel-lines? normalized)))))

  (testing "parallel-files without sequential-lines processes lines sequentially"
    (let [opts {:parallel-files true}
          normalized (input/normalize-input-options opts)]
      (is (true? (:parallel-files? normalized)))
      (is (false? (:parallel-lines? normalized))
          "files and lines aren't both parallel")))

  (testing "preserves file option"
    (let [opts {:file "test.md"}
          normalized (input/normalize-input-options opts)]
      (is (= "test.md" (:file normalized)))))

  (testing "preserves code-blocks option"
    (let [opts {:code-blocks true}
          normalized (input/normalize-input-options opts)]
      (is (true? (:code-blocks normalized)))))

  (testing "preserves quoted-text option"
    (let [opts {:quoted-text true}
          normalized (input/normalize-input-options opts)]
      (is (true? (:quoted-text normalized)))))

  (testing "preserves output option"
    (let [opts {:output "json"}
          normalized (input/normalize-input-options opts)]
      (is (= "json" (:output normalized)))))

  (testing "preserves no-cache option"
    (let [opts {:no-cache true}
          normalized (input/normalize-input-options opts)]
      (is (true? (:no-cache normalized)))))

  (testing "preserves skip-ignore option"
    (let [opts {:skip-ignore true}
          normalized (input/normalize-input-options opts)]
      (is (true? (:skip-ignore normalized)))))

  (testing "preserves all options together"
    (let [opts {:file "test.md" :code-blocks true :quoted-text true
                :output "json" :no-cache true :skip-ignore true
                :exclude ["*.log"] :parallel-files true :sequential-lines false
                :config {:checks []} :check-dir "/path/to/checks"}
          normalized (input/normalize-input-options opts)]
      (is (= "test.md" (:file normalized)))
      (is (true? (:code-blocks normalized)))
      (is (true? (:quoted-text normalized)))
      (is (= "json" (:output normalized)))
      (is (true? (:no-cache normalized)))
      (is (true? (:skip-ignore normalized)))
      (is (= ["*.log"] (:exclude-patterns normalized)))
      (is (true? (:parallel-files? normalized)))
      (is (false? (:sequential-lines? normalized)))
      (is (= {:checks []} (:config normalized)))
      (is (= "/path/to/checks" (:check-dir normalized))))))

;;; Tests for build-input-record (pure function extracted from make)

(deftest build-input-record-test
  (testing "builds Input record from normalized options and loaded data"
    (let [normalized {:file "test.md" :output "group" :no-cache false
                     :parallel-lines? true :config {:checks []}
                     :check-dir "/checks"}
          loaded {:lines [{:text "line1"}] :checks [{:name "check1"}]}
          cached {:some "cache"}
          project-ignore #{"ignore1"}
          project-ignore-issues #{1 2 3}
          input (input/build-input-record
                  {:normalized normalized
                   :loaded loaded
                   :cached cached
                   :project-ignore project-ignore
                   :project-ignore-issues project-ignore-issues})]
      (is (= "test.md" (:file input)))
      (is (= [{:text "line1"}] (:lines input)))
      (is (= [{:name "check1"}] (:checks input)))
      (is (= {:some "cache"} (:cached-result input)))
      (is (= "group" (:output input)))
      (is (false? (:no-cache input)))
      (is (true? (:parallel-lines input)))
      (is (= #{"ignore1"} (:project-ignore input)))
      (is (= #{1 2 3} (:project-ignore-issues input)))))

  (testing "handles nil cached result"
    (let [normalized {:file "test.md" :output "json" :no-cache true
                     :parallel-lines? false :config {:checks []}
                     :check-dir "/checks"}
          loaded {:lines [] :checks []}
          input (input/build-input-record
                  {:normalized normalized
                   :loaded loaded
                   :cached nil
                   :project-ignore #{}
                   :project-ignore-issues #{}})]
      (is (nil? (:cached-result input)))
      (is (= "test.md" (:file input)))
      (is (= [] (:lines input)))
      (is (= [] (:checks input)))))

  (testing "preserves config and check-dir from normalized options"
    (let [normalized {:file "test.md" :config {:checks ["check1"]}
                     :check-dir "/custom/checks" :output "verbose"
                     :no-cache false :parallel-lines? true}
          loaded {:lines [{:text "test"}] :checks [{:name "check1"}]}
          input (input/build-input-record
                  {:normalized normalized
                   :loaded loaded
                   :cached nil
                   :project-ignore #{}
                   :project-ignore-issues #{}})]
      (is (= {:checks ["check1"]} (:config input)))
      (is (= "/custom/checks" (:check-dir input)))))

  (testing "handles empty ignore sets"
    (let [normalized {:file "test.md" :output "group" :no-cache false
                     :parallel-lines? true :config {} :check-dir ""}
          loaded {:lines [] :checks []}
          input (input/build-input-record
                  {:normalized normalized
                   :loaded loaded
                   :cached nil
                   :project-ignore #{}
                   :project-ignore-issues #{}})]
      (is (= #{} (:project-ignore input)))
      (is (= #{} (:project-ignore-issues input)))))

  (testing "preserves all fields correctly"
    (let [normalized {:file "doc.md" :output "json" :no-cache true
                     :parallel-lines? false :config {:checks ["a" "b"]}
                     :check-dir "/path"}
          loaded {:lines [{:text "l1"} {:text "l2"}]
                 :checks [{:name "c1"} {:name "c2"}]}
          cached {:result "cached"}
          project-ignore #{"ign1" "ign2"}
          project-ignore-issues #{5 10 15}
          input (input/build-input-record
                  {:normalized normalized
                   :loaded loaded
                   :cached cached
                   :project-ignore project-ignore
                   :project-ignore-issues project-ignore-issues})]
      (is (= "doc.md" (:file input)))
      (is (= [{:text "l1"} {:text "l2"}] (:lines input)))
      (is (= {:checks ["a" "b"]} (:config input)))
      (is (= "/path" (:check-dir input)))
      (is (= [{:name "c1"} {:name "c2"}] (:checks input)))
      (is (= {:result "cached"} (:cached-result input)))
      (is (= "json" (:output input)))
      (is (true? (:no-cache input)))
      (is (false? (:parallel-lines input)))
      (is (= #{"ign1" "ign2"} (:project-ignore input)))
      (is (= #{5 10 15} (:project-ignore-issues input))))))

;;; Tests for combine-loaded-data (helper extracted from make)

(deftest combine-loaded-data-test
  (testing "combines lines and checks into Input record"
    (let [normalized {:file "test.md" :config {:checks []} :check-dir "/checks"
                     :output "group" :no-cache false :parallel-lines? true}
          lines [{:text "line1" :number 1}]
          loaded-checks {:checks [{:name "check1"}] :warnings []}
          project-ignore #{"ignore1"}
          project-ignore-issues #{1 2}
          input (input/combine-loaded-data normalized lines loaded-checks
                                          project-ignore project-ignore-issues)]
      (is (= "test.md" (:file input)))
      (is (= [{:text "line1" :number 1}] (:lines input)))
      (is (= [{:name "check1"}] (:checks input)))
      (is (= #{"ignore1"} (:project-ignore input)))
      (is (= #{1 2} (:project-ignore-issues input)))))

  (testing "handles empty lines and checks"
    (let [normalized {:file "empty.md" :config {} :check-dir ""
                     :output "json" :no-cache true :parallel-lines? false}
          lines []
          loaded-checks {:checks [] :warnings []}
          input (input/combine-loaded-data normalized lines loaded-checks #{} #{})]
      (is (= [] (:lines input)))
      (is (= [] (:checks input)))
      (is (= #{} (:project-ignore input)))
      (is (= #{} (:project-ignore-issues input)))))

  (testing "preserves all normalized config fields"
    (let [normalized {:file "doc.md" :config {:checks ["a"]} :check-dir "/custom"
                     :output "verbose" :no-cache true :parallel-lines? false}
          lines [{:text "test"}]
          loaded-checks {:checks [{:name "c"}] :warnings []}
          input (input/combine-loaded-data normalized lines loaded-checks
                                          #{"ign"} #{5})]
      (is (= {:checks ["a"]} (:config input)))
      (is (= "/custom" (:check-dir input)))
      (is (= "verbose" (:output input)))
      (is (true? (:no-cache input)))
      (is (false? (:parallel-lines input)))))

  (testing "retrieves cached result from storage"
    (let [normalized {:file "cached.md" :config {} :check-dir ""
                     :output "group" :no-cache false :parallel-lines? true}
          lines []
          loaded-checks {:checks [] :warnings []}]
      (with-redefs [proserunner.storage/get-cached-result
                    (fn [file _opts]
                      (is (= "cached.md" file))
                      (proserunner.result/ok {:cached "result"}))]
        (let [input (input/combine-loaded-data normalized lines loaded-checks #{} #{})]
          (is (= {:cached "result"} (:cached-result input))))))))
