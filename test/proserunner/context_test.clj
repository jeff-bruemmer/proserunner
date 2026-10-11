(ns proserunner.context-test
  "Tests for unified context resolution."
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.context :as context]
            [proserunner.test-helpers :refer [with-temp-dir]]
            [clojure.java.io :as io]))

(deftest determine-context-with-project-flag-test
  (testing "Force project scope with :project flag"
    (with-temp-dir [temp-dir "context-test"]
      ;; Create a project manifest
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:project true
                                                  :start-dir temp-dir})]
          (is (= :project (:target result)))
          (is (= "project" (:msg-context result)))
          (is (= temp-dir (:start-dir result))))))))

(deftest determine-context-with-global-flag-test
  (testing "Force global scope with :global flag"
    (with-temp-dir [temp-dir "context-test"]
      ;; Even with a project manifest present
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:global true
                                                  :start-dir temp-dir})]
          (is (= :global (:target result)))
          (is (= "global" (:msg-context result))))))))

(deftest determine-context-auto-detect-with-manifest-test
  (testing "Auto-detect project when manifest exists"
    (with-temp-dir [temp-dir "context-test"]
      ;; Create a project manifest
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:start-dir temp-dir})]
          (is (= :project (:target result)))
          (is (= "project" (:msg-context result))))))))

(deftest determine-context-auto-detect-without-manifest-test
  (testing "Auto-detect global when no manifest exists"
    (with-temp-dir [temp-dir "context-test"]
      ;; No .proserunner directory
      (let [result (context/determine-context {:start-dir temp-dir})]
        (is (= :global (:target result)))
        (is (= "global" (:msg-context result)))))))

(deftest determine-context-includes-project-root-test
  (testing "Includes project root when target is project (default)"
    (with-temp-dir [temp-dir "context-test"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:project true
                                                  :start-dir temp-dir})]
          (is (= :project (:target result)))
          (is (contains? result :project-root))
          (is (= temp-dir (:project-root result))))))))

(deftest determine-context-with-alt-msg-test
  (testing "Includes alternate scope message when requested for project"
    (with-temp-dir [temp-dir "context-test"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:project true
                                                  :start-dir temp-dir
                                                  :include-alt-msg true})]
          (is (= :project (:target result)))
          (is (contains? result :alt-msg))
          (is (= "Use --global to add to global ignore list instead."
                 (:alt-msg result)))))))

  (testing "Includes alternate scope message when requested for global"
    (with-temp-dir [temp-dir "context-test"]
      (let [result (context/determine-context {:global true
                                                :start-dir temp-dir
                                                :include-alt-msg true})]
        (is (= :global (:target result)))
        (is (contains? result :alt-msg))
        (is (= "Use --project to add to project ignore list instead."
               (:alt-msg result)))))))

(deftest determine-context-with-normalize-opts-test
  (testing "Includes normalized opts when requested for project"
    (with-temp-dir [temp-dir "context-test"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [opts {:project true :start-dir temp-dir :some-flag "value"}
              result (context/determine-context (assoc opts :normalize-opts true))]
          (is (= :project (:target result)))
          (is (contains? result :opts-with-target))
          (is (true? (:project (:opts-with-target result))))
          (is (= "value" (:some-flag (:opts-with-target result))))))))

  (testing "Includes normalized opts when requested for global"
    (with-temp-dir [temp-dir "context-test"]
      (let [opts {:global true :start-dir temp-dir :some-flag "value"}
            result (context/determine-context (assoc opts :normalize-opts true))]
        (is (= :global (:target result)))
        (is (contains? result :opts-with-target))
        (is (false? (:project (:opts-with-target result))))))))

(deftest determine-context-without-optional-features-test
  (testing "Returns minimal context when no optional features requested"
    (with-temp-dir [temp-dir "context-test"]
      (let [result (context/determine-context {:global true
                                                :start-dir temp-dir
                                                :include-alt-msg false
                                                :normalize-opts false})]
        (is (= :global (:target result)))
        (is (= "global" (:msg-context result)))
        (is (= temp-dir (:start-dir result)))
        (is (not (contains? result :project-root)))
        (is (not (contains? result :alt-msg)))
        (is (not (contains? result :opts-with-target)))))))

(deftest with-context-test
  (testing "with-context passes context to function"
    (with-temp-dir [temp-dir "context-test"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")]
        (.mkdirs proserunner-dir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (atom nil)]
          (context/with-context {:project true :start-dir temp-dir}
            (fn [ctx]
              (reset! result ctx)))

          (is (= :project (:target @result)))
          (is (= "project" (:msg-context @result)))
          (is (= temp-dir (:project-root @result)))))))

  (testing "with-context returns function result"
    (with-temp-dir [temp-dir "context-test"]
      (let [result (context/with-context {:global true :start-dir temp-dir}
                     (fn [{:keys [target]}]
                       (if (= target :global)
                         "global-result"
                         "project-result")))]
        (is (= "global-result" result))))))

(deftest determine-context-nested-project-detection-test
  (testing "Detects project from nested subdirectory"
    (with-temp-dir [temp-dir "context-test"]
      (let [proserunner-dir (io/file temp-dir ".proserunner")
            subdir (io/file temp-dir "src" "nested")]
        (.mkdirs proserunner-dir)
        (.mkdirs subdir)
        (spit (io/file proserunner-dir "config.edn") "{:checks []}")

        (let [result (context/determine-context {:start-dir (.getAbsolutePath subdir)})]
          (is (= :project (:target result)))
          (is (= temp-dir (:project-root result))))))))
